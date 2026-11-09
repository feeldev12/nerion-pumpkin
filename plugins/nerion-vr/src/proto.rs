//! `nerion:vr`: where a VR player's head and hands are, on the wire (format v1, nerion-vr.md
//! "Formato v1"). The same bytes as NerionServerSDK's and Nerion VR's Java codecs: the shared
//! vectors in `tests/nerion_vr_pose_cases.json` (made by NerionServerSDK's
//! `scripts/nerion_vr_pose_cases.py`) check all three.
//!
//! Positions are metres from the player's feet in the body frame: +Y up, +Z the body's facing,
//! +X the player's left (at body yaw 0 the frame is the world's axes). Rotations are unit
//! quaternions `[x, y, z, w]` in that frame, identity looking along +Z with +Y up. Doubles and
//! `floor(x + 0.5)` rounding everywhere, so every language writes the same bytes.

/// The custom payload channel.
pub const CHANNEL: &str = "nerion:vr";
pub const VERSION: u8 = 1;
/// Header only: the player is not in VR.
pub const MIN_SIZE: usize = 3;
/// Header, yaw, eye height, head and both hands.
pub const MAX_SIZE: usize = 37;
pub const POSITION_SCALE: f64 = 8192.0;

const VR: u8 = 1;
const LEFT_HANDED: u8 = 2;
const SEATED: u8 = 4;
const CROUCH: u8 = 8;
const LEFT: u8 = 16;
const RIGHT: u8 = 32;
const EYE_HEIGHT: u8 = 64;
const INV_SQRT2: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// A tracked device: position (metres) and rotation `[x, y, z, w]` in the body frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tracked {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
}

impl Tracked {
    pub fn distance_to(&self, other: &Tracked) -> f64 {
        let d = |i: usize| f64::from(self.pos[i]) - f64::from(other.pos[i]);
        (d(0) * d(0) + d(1) * d(1) + d(2) * d(2)).sqrt()
    }
}

/// One pose. `vr_active == false`: the player left VR (receivers clear the pose).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub sequence: u8,
    pub vr_active: bool,
    pub left_handed: bool,
    pub seated: bool,
    pub crouching: bool,
    /// Degrees, Minecraft's yaw (0 = facing +Z).
    pub body_yaw: f32,
    /// The calibrated real eye height in mm, if sent this frame.
    pub eye_height_mm: Option<u16>,
    pub head: Option<Tracked>,
    pub left: Option<Tracked>,
    pub right: Option<Tracked>,
}

impl Pose {
    pub fn not_in_vr(sequence: u8) -> Self {
        Pose {
            sequence,
            vr_active: false,
            left_handed: false,
            seated: false,
            crouching: false,
            body_yaw: 0.0,
            eye_height_mm: None,
            head: None,
            left: None,
            right: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Size(usize),
    Version(u8),
    ReservedFlag,
    NoHead,
}

fn round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

fn yaw_u16(deg: f64) -> u16 {
    let mut y = deg % 360.0;
    if y < 0.0 {
        y += 360.0;
    }
    (round(y / 360.0 * 65536.0) & 0xFFFF) as u16
}

fn pos_i16(m: f64) -> i16 {
    round(m * POSITION_SCALE).clamp(-32768, 32767) as i16
}

fn quat_u32(q: [f32; 4]) -> u32 {
    let q = q.map(f64::from);
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let mut c = q.map(|v| v / n);
    let mut big = 0;
    for i in 1..4 {
        if c[i].abs() > c[big].abs() {
            big = i;
        }
    }
    if c[big] < 0.0 {
        c = c.map(|v| -v);
    }
    let mut out = (big as u32) << 30;
    let mut shift = 20;
    for (i, v) in c.iter().enumerate() {
        if i == big {
            continue;
        }
        let q = round(v / INV_SQRT2 * 511.5 + 511.5).clamp(0, 1023) as u32;
        out |= q << shift;
        shift -= 10;
    }
    out
}

fn u32_quat(packed: u32) -> [f32; 4] {
    let big = ((packed >> 30) & 3) as usize;
    let mut c = [0.0f64; 4];
    let mut shift = 20;
    let mut sum = 0.0;
    for (i, slot) in c.iter_mut().enumerate() {
        if i == big {
            continue;
        }
        let v = f64::from((packed >> shift) & 1023);
        *slot = (v - 511.5) / 511.5 * INV_SQRT2;
        sum += *slot * *slot;
        shift -= 10;
    }
    c[big] = (1.0 - sum).max(0.0).sqrt();
    c.map(|v| v as f32)
}

/// The exact payload size a flags byte announces.
pub fn size_for(flags: u8) -> usize {
    if flags & VR == 0 {
        return MIN_SIZE;
    }
    let mut size = MIN_SIZE + 2 + 10;
    if flags & EYE_HEIGHT != 0 {
        size += 2;
    }
    if flags & LEFT != 0 {
        size += 10;
    }
    if flags & RIGHT != 0 {
        size += 10;
    }
    size
}

fn put_tracked(out: &mut Vec<u8>, t: &Tracked) {
    for p in t.pos {
        out.extend_from_slice(&pos_i16(f64::from(p)).to_be_bytes());
    }
    out.extend_from_slice(&quat_u32(t.rot).to_be_bytes());
}

/// The payload's bytes. A VR pose without a head writes none (a head is required).
pub fn encode(p: &Pose) -> Vec<u8> {
    let vr = p.vr_active && p.head.is_some();
    let mut flags = 0u8;
    if vr {
        flags |= VR;
    }
    if p.left_handed {
        flags |= LEFT_HANDED;
    }
    if p.seated {
        flags |= SEATED;
    }
    if p.crouching {
        flags |= CROUCH;
    }
    if vr && p.left.is_some() {
        flags |= LEFT;
    }
    if vr && p.right.is_some() {
        flags |= RIGHT;
    }
    if vr && p.eye_height_mm.is_some() {
        flags |= EYE_HEIGHT;
    }
    let mut out = Vec::with_capacity(MAX_SIZE);
    out.extend_from_slice(&[VERSION, flags, p.sequence]);
    if vr {
        out.extend_from_slice(&yaw_u16(f64::from(p.body_yaw)).to_be_bytes());
        if let Some(h) = p.eye_height_mm {
            out.extend_from_slice(&h.to_be_bytes());
        }
        for t in [p.head, p.left, p.right].iter().flatten() {
            put_tracked(&mut out, t);
        }
    }
    out
}

fn tracked_at(d: &[u8], at: &mut usize) -> Tracked {
    let i16_at = |o: usize| f32::from(i16::from_be_bytes([d[o], d[o + 1]])) / POSITION_SCALE as f32;
    let pos = [i16_at(*at), i16_at(*at + 2), i16_at(*at + 4)];
    let rot = u32_quat(u32::from_be_bytes([
        d[*at + 6],
        d[*at + 7],
        d[*at + 8],
        d[*at + 9],
    ]));
    *at += 10;
    Tracked { pos, rot }
}

/// A well-formed v1 payload's pose.
pub fn decode(d: &[u8]) -> Result<Pose, DecodeError> {
    if d.len() < MIN_SIZE || d.len() > MAX_SIZE {
        return Err(DecodeError::Size(d.len()));
    }
    if d[0] != VERSION {
        return Err(DecodeError::Version(d[0]));
    }
    let flags = d[1];
    if flags & 0x80 != 0 {
        return Err(DecodeError::ReservedFlag);
    }
    if d.len() != size_for(flags) {
        return Err(DecodeError::Size(d.len()));
    }
    let mut pose = Pose::not_in_vr(d[2]);
    pose.left_handed = flags & LEFT_HANDED != 0;
    pose.seated = flags & SEATED != 0;
    pose.crouching = flags & CROUCH != 0;
    if flags & VR == 0 {
        return Ok(pose);
    }
    pose.vr_active = true;
    pose.body_yaw = (f64::from(u16::from_be_bytes([d[3], d[4]])) * 360.0 / 65536.0) as f32;
    let mut at = 5;
    if flags & EYE_HEIGHT != 0 {
        pose.eye_height_mm = Some(u16::from_be_bytes([d[5], d[6]]));
        at = 7;
    }
    pose.head = Some(tracked_at(d, &mut at));
    if flags & LEFT != 0 {
        pose.left = Some(tracked_at(d, &mut at));
    }
    if flags & RIGHT != 0 {
        pose.right = Some(tracked_at(d, &mut at));
    }
    Ok(pose)
}

/// What a server checks and forwards, the same numbers as NerionServerSDK's `VrRelayRules`.
pub mod relay {
    use super::Pose;

    pub const MAX_PER_SECOND: f64 = 25.0;
    pub const BURST: f64 = 10.0;
    pub const MAX_DISTANCE: f64 = 128.0;
    pub const MAX_HAND_REACH: f64 = 1.5;
    pub const MIN_HEAD_HEIGHT: f64 = 0.2;
    pub const MAX_HEAD_HEIGHT: f64 = 2.8;
    pub const MAX_HEAD_OFFSET: f64 = 2.0;

    /// `None` when the pose is a body, else why it's dropped.
    pub fn reject(p: &Pose) -> Option<&'static str> {
        if !p.vr_active {
            return None;
        }
        let head = p.head?;
        let y = f64::from(head.pos[1]);
        if !(MIN_HEAD_HEIGHT..=MAX_HEAD_HEIGHT).contains(&y) {
            return Some("head height");
        }
        if f64::from(head.pos[0]).hypot(f64::from(head.pos[2])) > MAX_HEAD_OFFSET {
            return Some("head offset");
        }
        if p.left
            .is_some_and(|h| h.distance_to(&head) > MAX_HAND_REACH)
        {
            return Some("left hand reach");
        }
        if p.right
            .is_some_and(|h| h.distance_to(&head) > MAX_HAND_REACH)
        {
            return Some("right hand reach");
        }
        None
    }

    /// Forward pose `sequence` to a viewer `distance` blocks away? All within 16, one in two
    /// within 48, one in four beyond, none past [`MAX_DISTANCE`]; leaving VR always goes.
    pub fn forward(sequence: u8, distance: f64, vr_active: bool) -> bool {
        if !vr_active {
            return true;
        }
        if distance > MAX_DISTANCE {
            return false;
        }
        if distance <= 16.0 {
            return true;
        }
        if distance <= 48.0 {
            return sequence & 1 == 0;
        }
        sequence & 3 == 0
    }

    /// Per-sender token bucket.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct RateLimit {
        tokens: Option<f64>,
        last: Option<std::time::Instant>,
    }

    impl RateLimit {
        pub fn try_take(&mut self, now: std::time::Instant) -> bool {
            let mut tokens = self.tokens.unwrap_or(BURST);
            if let Some(last) = self.last {
                tokens = (tokens
                    + now.saturating_duration_since(last).as_secs_f64() * MAX_PER_SECOND)
                    .min(BURST);
            }
            self.last = Some(now);
            if tokens < 1.0 {
                self.tokens = Some(tokens);
                return false;
            }
            self.tokens = Some(tokens - 1.0);
            true
        }
    }
}

/// The body frame into Animorph's model space (GeckoLib pixels from the feet, front -Z, the
/// player's left at -X: 180° about Y), as `mobs::rig::vr::VrPose` takes it.
pub mod model {
    pub const PIXELS_PER_METRE: f32 = 16.0;

    pub fn position(p: [f32; 3]) -> [f32; 3] {
        [
            -p[0] * PIXELS_PER_METRE,
            p[1] * PIXELS_PER_METRE,
            -p[2] * PIXELS_PER_METRE,
        ]
    }

    pub fn rotation(q: [f32; 4]) -> [f32; 4] {
        [-q[0], q[1], -q[2], q[3]]
    }
}

/// A forwarded pose (server to client): the sender's entity id as a VarInt, then its bytes.
pub fn forwarded(entity_id: i32, pose_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + pose_bytes.len());
    let mut v = entity_id as u32;
    loop {
        if v & !0x7F == 0 {
            out.push(v as u8);
            break;
        }
        out.push((v & 0x7F | 0x80) as u8);
        v >>= 7;
    }
    out.extend_from_slice(pose_bytes);
    out
}

/// Splits a forwarded pose back into the entity id and the pose bytes.
pub fn split_forwarded(d: &[u8]) -> Option<(i32, &[u8])> {
    let mut value = 0u32;
    for (i, b) in d.iter().enumerate().take(5) {
        value |= u32::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            return Some((value as i32, &d[i + 1..]));
        }
    }
    None
}
