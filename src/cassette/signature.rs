use blake3::Hasher;
use serde_json::Value;

pub fn compute_request_signature(
    method: &str,
    path: &str,
    body: Option<&Value>,
    raw_body: Option<&[u8]>,
) -> String {
    let mut hasher = Hasher::new();
    hasher.update(method.to_uppercase().as_bytes());
    hasher.update(b":");

    // Normalize path by stripping trailing slash
    let normalized_path = path.trim_end_matches('/');
    hasher.update(normalized_path.as_bytes());
    hasher.update(b":");

    if let Some(json_val) = body {
        let canonical_bytes = canonicalize_json(json_val);
        hasher.update(&canonical_bytes);
    } else if let Some(raw) = raw_body {
        hasher.update(raw);
    }

    hasher.finalize().to_hex().to_string()
}

/// Recursively canonicalizes JSON by sorting object keys and serializing compact
pub fn canonicalize_json(value: &Value) -> Vec<u8> {
    match value {
        Value::Object(map) => {
            let mut sorted_keys: Vec<&String> = map.keys().collect();
            sorted_keys.sort();

            let mut out = Vec::new();
            out.push(b'{');
            for (i, key) in sorted_keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(
                    serde_json::to_string(key)
                        .unwrap_or_else(|_| format!("\"{}\"", key))
                        .as_bytes(),
                );
                out.push(b':');
                out.extend_from_slice(&canonicalize_json(&map[*key]));
            }
            out.push(b'}');
            out
        }
        Value::Array(arr) => {
            let mut out = Vec::new();
            out.push(b'[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(&canonicalize_json(item));
            }
            out.push(b']');
            out
        }
        other => serde_json::to_vec(other).unwrap_or_default(),
    }
}
