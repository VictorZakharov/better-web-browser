//! Convert validated CSS filter operations to private bootstrap values.
use super::*;
use crate::engine::css::filter_functions::{self, Argument, Environment, Operation};

pub(in crate::engine::script) fn parse(source: &str, environment: Environment) -> JsValue {
    filter_functions::parse(source, environment).map_or(JsValue::Null, |operations| {
        JsValue::Array(operations.into_iter().map(operation).collect())
    })
}

fn operation(operation: Operation) -> JsValue {
    let value = match operation.value {
        Argument::Scalar(value) => JsValue::from(f64::from(value)),
        Argument::Shadow {
            x,
            y,
            blur,
            channels,
            origin_clean,
        } => JsValue::Object(vec![
            ("originClean".into(), JsValue::Boolean(origin_clean)),
            ("x".into(), JsValue::from(f64::from(x))),
            ("y".into(), JsValue::from(f64::from(y))),
            ("blur".into(), JsValue::from(f64::from(blur))),
            (
                "color".into(),
                JsValue::Object(vec![(
                    "channels".into(),
                    JsValue::Array(
                        channels
                            .into_iter()
                            .map(|byte| JsValue::from(u32::from(byte)))
                            .collect(),
                    ),
                )]),
            ),
        ]),
    };
    JsValue::Object(vec![
        ("name".into(), JsValue::from(operation.name.to_owned())),
        ("value".into(), value),
    ])
}

pub(super) fn worker(args: &[JsValue]) -> JsValue {
    let Some(JsValue::String(source)) = args.get(2) else {
        return JsValue::Null;
    };
    parse(source, Environment::default())
}
