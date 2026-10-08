use super::*;
use crate::engine::css::values::math::{Function, Rounding};

impl Budget {
    pub(super) fn extended_function<'i>(
        &mut self,
        input: &mut Parser<'i, '_>,
        name: &str,
        depth: usize,
    ) -> Result<'i, Value> {
        let function = match name {
            "round" => {
                let strategy = input
                    .try_parse(|p| -> Result<'i, Rounding> {
                        let name = p.expect_ident()?.to_ascii_lowercase();
                        let strategy = match name.as_str() {
                            "nearest" => Rounding::Nearest,
                            "up" => Rounding::Up,
                            "down" => Rounding::Down,
                            "to-zero" => Rounding::ToZero,
                            _ => return Err(p.new_custom_error(())),
                        };
                        p.expect_comma()?;
                        Ok(strategy)
                    })
                    .unwrap_or(Rounding::Nearest);
                Function::Round(strategy)
            }
            "mod" => Function::Mod,
            "rem" => Function::Rem,
            "abs" => Function::Abs,
            "sign" => Function::Sign,
            "hypot" => Function::Hypot,
            "pow" => Function::Pow,
            "sqrt" => Function::Sqrt,
            "log" => Function::Log,
            "exp" => Function::Exp,
            "sin" => Function::Sin,
            "cos" => Function::Cos,
            "tan" => Function::Tan,
            "asin" => Function::Asin,
            "acos" => Function::Acos,
            "atan" => Function::Atan,
            "atan2" => Function::Atan2,
            _ => return Err(input.new_custom_error(())),
        };
        let mut values = input.parse_comma_separated(|p| self.sum(p, depth))?;
        let kind = values
            .first()
            .ok_or_else(|| input.new_custom_error(()))?
            .kind();
        let valid_arity = match function {
            Function::Round(_) => values.len() == 2 || kind == Kind::Number && values.len() == 1,
            Function::Mod | Function::Rem | Function::Pow | Function::Atan2 => values.len() == 2,
            Function::Log => matches!(values.len(), 1 | 2),
            Function::Hypot => !values.is_empty(),
            _ => values.len() == 1,
        };
        if !valid_arity || values.iter().any(|value| value.kind() != kind) {
            return Err(input.new_custom_error(()));
        }
        if matches!(
            function,
            Function::Pow
                | Function::Sqrt
                | Function::Log
                | Function::Exp
                | Function::Asin
                | Function::Acos
                | Function::Atan
        ) && kind != Kind::Number
        {
            return Err(input.new_custom_error(()));
        }
        if matches!(function, Function::Sin | Function::Cos | Function::Tan)
            && !matches!(kind, Kind::Number | Kind::Angle)
        {
            return Err(input.new_custom_error(()));
        }
        if matches!(function, Function::Round(_)) && values.len() == 1 {
            values.push(Value::Number(self.node(input, Expression::Number(1.0))?));
        }
        let expression = self.node(
            input,
            Expression::Function(
                function,
                values.into_iter().map(Value::expression).collect(),
            ),
        )?;
        let result_kind = match function {
            Function::Sign | Function::Sin | Function::Cos | Function::Tan => Kind::Number,
            Function::Asin | Function::Acos | Function::Atan | Function::Atan2 => Kind::Angle,
            _ => kind,
        };
        Ok(Value::typed(result_kind, expression))
    }
}
