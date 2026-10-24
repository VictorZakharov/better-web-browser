//! JSON cannot carry Web IDL unrestricted floats directly. A closed spelling
//! keeps NaN/infinities numeric at the private bridge without accepting objects.
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

pub(super) fn deserialize<'de, D: Deserializer<'de>>(reader: D) -> Result<Vec<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Float {
        Number(f64),
        Special(String),
    }
    Vec::<Float>::deserialize(reader)?
        .into_iter()
        .map(|value| match value {
            Float::Number(value) => Ok(value),
            Float::Special(value) => match value.as_str() {
                "nan" => Ok(f64::NAN),
                "inf" => Ok(f64::INFINITY),
                "-inf" => Ok(f64::NEG_INFINITY),
                "-0" => Ok(-0.0),
                _ => Err(serde::de::Error::custom("Invalid WebGL float encoding")),
            },
        })
        .collect()
}

pub(super) fn encode(value: f32) -> Value {
    if value == 0.0 && value.is_sign_negative() {
        json!({"webglFloat":"-0"})
    } else if value.is_finite() {
        json!(value)
    } else {
        json!({"webglFloat":if value.is_nan() { "nan" } else if value > 0.0 { "inf" } else { "-inf" }})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Values {
        #[serde(deserialize_with = "deserialize")]
        f: Vec<f64>,
    }
    #[test]
    fn closed_float_encoding_preserves_unrestricted_numbers() {
        let values: Values =
            serde_json::from_str(r#"{"f":[0,-0.0,1.5,"nan","inf","-inf"]}"#).unwrap();
        assert_eq!(values.f[0], 0.0);
        assert!(values.f[1].is_sign_negative());
        assert_eq!(values.f[2], 1.5);
        assert!(values.f[3].is_nan());
        assert_eq!(values.f[4], f64::INFINITY);
        assert_eq!(values.f[5], f64::NEG_INFINITY);
        for text in [
            r#"{"f":[null]}"#,
            r#"{"f":[{}]}"#,
            r#"{"f":["1.5"]}"#,
            r#"{"f":["NaN"]}"#,
            r#"{"f":[true]}"#,
        ] {
            assert!(serde_json::from_str::<Values>(text).is_err());
        }
        assert_eq!(encode(f32::NAN), json!({"webglFloat":"nan"}));
        assert_eq!(encode(f32::INFINITY), json!({"webglFloat":"inf"}));
        assert_eq!(encode(f32::NEG_INFINITY), json!({"webglFloat":"-inf"}));
        assert_eq!(encode(1.5), json!(1.5));
    }
}
