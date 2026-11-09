//! The shared vectors: the same bytes as the Python reference and both Java codecs.
use nerion_vr_pumpkin::proto::{Pose, Tracked, decode, encode, forwarded, relay, split_forwarded};
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("nerion_vr_pose_cases.json")).expect("vectors")
}

fn floats<const N: usize>(v: &Value) -> [f32; N] {
    let a = v.as_array().expect("array");
    std::array::from_fn(|i| a[i].as_f64().expect("number") as f32)
}

fn tracked(c: &Value, key: &str) -> Option<Tracked> {
    let t = c.get(key)?;
    if t.is_null() {
        return None;
    }
    Some(Tracked {
        pos: floats(&t["pos"]),
        rot: floats(&t["rot"]),
    })
}

fn flag(c: &Value, key: &str) -> bool {
    c.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn input(c: &Value) -> Pose {
    Pose {
        sequence: c["seq"].as_u64().expect("seq") as u8,
        vr_active: flag(c, "vr"),
        left_handed: flag(c, "leftHanded"),
        seated: flag(c, "seated"),
        crouching: flag(c, "crouch"),
        body_yaw: c.get("bodyYaw").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        eye_height_mm: c
            .get("eyeHeightMm")
            .and_then(Value::as_u64)
            .map(|v| v as u16),
        head: tracked(c, "head"),
        left: tracked(c, "left"),
        right: tracked(c, "right"),
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

#[test]
fn encodes_the_shared_vectors_bit_for_bit() {
    for c in vectors()["cases"].as_array().expect("cases") {
        let name = c["name"].as_str().expect("name");
        let bytes = encode(&input(c));
        assert_eq!(hex(&bytes), c["hex"].as_str().expect("hex"), "{name}");
        assert_eq!(
            bytes.len() as u64,
            c["size"].as_u64().expect("size"),
            "{name}"
        );
    }
}

#[test]
fn decodes_the_shared_vectors_back_within_precision() {
    let v = vectors();
    let pos_tol = v["tolerances"]["positionM"].as_f64().expect("tol");
    let yaw_tol = v["tolerances"]["yawDeg"].as_f64().expect("tol");
    let dot_min = v["tolerances"]["rotationDot"].as_f64().expect("tol");
    let limit = 32767.0 / nerion_vr_pumpkin::proto::POSITION_SCALE;
    for c in v["cases"].as_array().expect("cases") {
        let name = c["name"].as_str().expect("name");
        let want = input(c);
        let got = decode(&unhex(c["hex"].as_str().expect("hex"))).expect(name);
        assert_eq!(
            (
                want.sequence,
                want.vr_active,
                want.left_handed,
                want.seated,
                want.crouching
            ),
            (
                got.sequence,
                got.vr_active,
                got.left_handed,
                got.seated,
                got.crouching
            ),
            "{name}"
        );
        if !want.vr_active {
            continue;
        }
        let wy = (f64::from(want.body_yaw) % 360.0 + 360.0) % 360.0;
        let dy = (wy - f64::from(got.body_yaw)).abs();
        assert!(dy.min(360.0 - dy) <= yaw_tol, "{name} yaw");
        assert_eq!(want.eye_height_mm, got.eye_height_mm, "{name}");
        for (w, g) in [
            (want.head, got.head),
            (want.left, got.left),
            (want.right, got.right),
        ] {
            assert_eq!(w.is_some(), g.is_some(), "{name}");
            let (Some(w), Some(g)) = (w, g) else { continue };
            for i in 0..3 {
                let wv = f64::from(w.pos[i]).clamp(-limit - 1.0 / 8192.0, limit);
                assert!(
                    (wv - f64::from(g.pos[i])).abs() <= pos_tol,
                    "{name} pos {i}"
                );
            }
            let n = w
                .rot
                .iter()
                .map(|v| f64::from(*v).powi(2))
                .sum::<f64>()
                .sqrt();
            let dot: f64 = (0..4)
                .map(|i| f64::from(w.rot[i]) * f64::from(g.rot[i]))
                .sum::<f64>()
                / n;
            assert!(dot.abs() >= dot_min, "{name} rotation dot {dot}");
        }
    }
}

#[test]
fn rejects_malformed_payloads() {
    assert!(decode(&[1, 0]).is_err());
    assert!(decode(&[2, 0, 0]).is_err());
    assert!(decode(&[1, 0x80, 0]).is_err());
    assert!(decode(&[1, 0, 0, 9]).is_err());
    let mut short_right = [0u8; 15];
    short_right[0] = 1;
    short_right[1] = 1 | 32;
    assert!(decode(&short_right).is_err());
    assert!(decode(&[0u8; 38]).is_err());
}

#[test]
fn relay_rules_match_the_sdk() {
    let at = |x: f32, y: f32, z: f32| Tracked {
        pos: [x, y, z],
        rot: [0.0, 0.0, 0.0, 1.0],
    };
    let mut p = Pose::not_in_vr(0);
    p.vr_active = true;
    p.head = Some(at(0.0, 1.62, 0.0));
    p.left = Some(at(0.3, 1.0, 0.3));
    assert_eq!(relay::reject(&p), None);
    p.left = Some(at(0.0, 1.6, 2.0));
    assert_eq!(relay::reject(&p), Some("left hand reach"));
    p.head = Some(at(0.0, 3.5, 0.0));
    assert_eq!(relay::reject(&p), Some("head height"));
    let counts = [10.0, 30.0, 100.0, 200.0]
        .map(|d| (0..=255u8).filter(|s| relay::forward(*s, d, true)).count());
    assert_eq!(counts, [256, 128, 64, 0]);
    assert!(relay::forward(1, 500.0, false));
    let mut limit = relay::RateLimit::default();
    let t0 = std::time::Instant::now();
    let accepted = (0..200)
        .filter(|i| limit.try_take(t0 + std::time::Duration::from_millis(50 * i)))
        .count();
    assert_eq!(accepted, 200);
}

#[test]
fn forwarded_wraps_the_entity_id() {
    for id in [0, 1, 127, 128, 300, 2_000_000, i32::MAX] {
        let d = forwarded(id, &[1, 0, 7]);
        let (got, rest) = split_forwarded(&d).expect("varint");
        assert_eq!((got, rest), (id, &[1u8, 0, 7][..]));
    }
}

#[test]
fn model_space_matches_nerion_vrs_java() {
    // Same numbers as Nerion VR's VrPoseFramesTest: ahead (+Z) is the model's front (-Z), the
    // left (+X) its -X, 16 px a metre; a nod about +X becomes one about -X.
    let p = nerion_vr_pumpkin::proto::model::position([0.3, 1.62, 0.5]);
    for (got, want) in p.iter().zip([-4.8f32, 25.92, -8.0]) {
        assert!((got - want).abs() < 1e-4, "{p:?}");
    }
    let s = (0.2f32).sin();
    let q = nerion_vr_pumpkin::proto::model::rotation([s, 0.0, 0.0, (0.2f32).cos()]);
    assert_eq!(q[0], -s);
}
