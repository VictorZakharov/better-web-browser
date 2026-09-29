//! Browser-owned top-level screen wake-lock policy admission.
//!
//! RFC 8941 dictionaries overwrite duplicate keys with the last member. A
//! Permissions Policy inner list with no recognized allowlist items denies.
//! Malformed fields fail closed for this privileged operation.
//! https://www.rfc-editor.org/rfc/rfc8941.html#section-4.2.2
//! https://www.w3.org/TR/permissions-policy-1/#construct-policy-from-dictionary-and-origin

use better_web_browser::fetch::HeaderList;

pub(in crate::windows_app) fn screen_wake_lock_allowed(headers: &HeaderList) -> bool {
    let mut last = None;
    for field in headers.values("permissions-policy") {
        match parse_dictionary(field) {
            Ok(Some(value)) => last = Some(value),
            Ok(None) => {}
            Err(()) => return false,
        }
    }
    // An unsupported scalar member is ignored by Permissions Policy; the
    // feature's default allowlist is `self` for this top-level document.
    last.flatten().unwrap_or(true)
}

#[derive(Clone, Copy)]
enum Item<'a> {
    Token(&'a str),
    String,
    Other,
}

struct Reader<'a> {
    input: &'a [u8],
    offset: usize,
}

fn parse_dictionary(field: &str) -> Result<Option<Option<bool>>, ()> {
    let mut reader = Reader {
        input: field.as_bytes(),
        offset: 0,
    };
    let mut last = None;
    reader.skip_ows();
    while !reader.done() {
        let key = reader.key()?;
        let value = if reader.take(b'=') {
            reader.member()?
        } else {
            reader.parameters()?;
            None // Implicit true is not a Permissions Policy allowlist.
        };
        if key == "screen-wake-lock" {
            last = Some(value);
        }
        reader.skip_ows();
        if reader.done() {
            break;
        }
        reader.expect(b',')?;
        reader.skip_ows();
        if reader.done() {
            return Err(());
        }
    }
    Ok(last)
}

impl<'a> Reader<'a> {
    fn done(&self) -> bool {
        self.offset == self.input.len()
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.offset).copied()
    }

    fn take(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ()> {
        self.take(byte).then_some(()).ok_or(())
    }

    fn skip_ows(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.offset += 1;
        }
    }

    fn skip_spaces(&mut self) {
        while self.take(b' ') {}
    }

    fn key(&mut self) -> Result<&'a str, ()> {
        let start = self.offset;
        if !matches!(self.peek(), Some(b'a'..=b'z' | b'*')) {
            return Err(());
        }
        self.offset += 1;
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'*')
        ) {
            self.offset += 1;
        }
        std::str::from_utf8(&self.input[start..self.offset]).map_err(|_| ())
    }

    fn member(&mut self) -> Result<Option<bool>, ()> {
        if self.take(b'(') {
            let mut allows = false;
            loop {
                self.skip_spaces();
                if self.take(b')') {
                    break;
                }
                let item = self.item()?;
                allows |= matches!(item, Item::Token("self" | "*"));
                if !matches!(self.peek(), Some(b' ' | b')')) {
                    return Err(());
                }
            }
            self.parameters()?;
            return Ok(Some(allows));
        }
        let item = self.item()?;
        self.parameters()?;
        Ok(match item {
            Item::Token("self" | "*") => Some(true),
            // Source-expression matching remains outside this admission slice.
            // A quoted expression is not treated as an implicit grant.
            Item::String => Some(false),
            Item::Token(_) | Item::Other => None,
        })
    }

    fn item(&mut self) -> Result<Item<'a>, ()> {
        let item = self.bare_item()?;
        self.parameters()?;
        Ok(item)
    }

    fn parameters(&mut self) -> Result<(), ()> {
        while self.take(b';') {
            self.skip_spaces();
            self.key()?;
            if self.take(b'=') {
                self.bare_item()?;
            }
        }
        Ok(())
    }

    fn bare_item(&mut self) -> Result<Item<'a>, ()> {
        match self.peek().ok_or(())? {
            b'"' => {
                self.string()?;
                Ok(Item::String)
            }
            b'-' | b'0'..=b'9' => {
                self.number()?;
                Ok(Item::Other)
            }
            b'?' => {
                self.offset += 1;
                if !matches!(self.peek(), Some(b'0' | b'1')) {
                    return Err(());
                }
                self.offset += 1;
                Ok(Item::Other)
            }
            b':' => {
                self.binary()?;
                Ok(Item::Other)
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'*' => self.token(),
            _ => Err(()),
        }
    }

    fn token(&mut self) -> Result<Item<'a>, ()> {
        let start = self.offset;
        self.offset += 1;
        while self.peek().is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~:/".contains(&byte)
        }) {
            self.offset += 1;
        }
        let token = std::str::from_utf8(&self.input[start..self.offset]).map_err(|_| ())?;
        Ok(Item::Token(token))
    }

    fn number(&mut self) -> Result<(), ()> {
        self.take(b'-');
        let start = self.offset;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.offset += 1;
        }
        let digits = self.offset - start;
        if digits == 0 || digits > 15 {
            return Err(());
        }
        if self.take(b'.') {
            if digits > 12 {
                return Err(());
            }
            let fraction = self.offset;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.offset += 1;
            }
            if !(1..=3).contains(&(self.offset - fraction)) {
                return Err(());
            }
        }
        Ok(())
    }

    fn string(&mut self) -> Result<(), ()> {
        self.expect(b'"')?;
        loop {
            let byte = self.peek().ok_or(())?;
            self.offset += 1;
            match byte {
                b'"' => return Ok(()),
                b'\\' => {
                    let escaped = self.peek().ok_or(())?;
                    if !matches!(escaped, b'"' | b'\\') {
                        return Err(());
                    }
                    self.offset += 1;
                }
                b' '..=b'~' => {}
                _ => return Err(()),
            }
        }
    }

    fn binary(&mut self) -> Result<(), ()> {
        self.expect(b':')?;
        while let Some(byte) = self.peek() {
            self.offset += 1;
            if byte == b':' {
                return Ok(());
            }
            if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')) {
                return Err(());
            }
        }
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_actual_empty_screen_wake_lock_member_denies() {
        let mut headers = HeaderList::new();
        assert!(screen_wake_lock_allowed(&headers));
        headers
            .append(
                "permissions-policy",
                "geolocation=(), screen-wake-lock=(self)",
            )
            .unwrap();
        assert!(screen_wake_lock_allowed(&headers));
        headers
            .append(
                "Permissions-Policy",
                "screen-wake-lock = ( ) ;report-to=power",
            )
            .unwrap();
        assert!(!screen_wake_lock_allowed(&headers));
    }

    #[test]
    fn quoted_commas_and_similar_feature_names_cannot_forge_a_denial() {
        let mut headers = HeaderList::new();
        headers
            .append(
                "permissions-policy",
                "other=(\"a, screen-wake-lock=()\"), x-screen-wake-lock=()",
            )
            .unwrap();
        assert!(screen_wake_lock_allowed(&headers));
        headers
            .append("permissions-policy", "screen-wake-lock=()")
            .unwrap();
        assert!(!screen_wake_lock_allowed(&headers));
    }

    #[test]
    fn last_duplicate_member_controls_admission_across_fields() {
        let mut headers = HeaderList::new();
        headers
            .append(
                "permissions-policy",
                "screen-wake-lock=(), screen-wake-lock=(self)",
            )
            .unwrap();
        assert!(screen_wake_lock_allowed(&headers));
        headers
            .append("permissions-policy", "screen-wake-lock=()")
            .unwrap();
        assert!(!screen_wake_lock_allowed(&headers));
        headers
            .append("permissions-policy", "screen-wake-lock=*")
            .unwrap();
        assert!(screen_wake_lock_allowed(&headers));
    }

    #[test]
    fn unrecognized_inner_list_items_cannot_create_an_allowance() {
        for value in [
            "screen-wake-lock=(42)",
            "screen-wake-lock=(?1 unknown)",
            "screen-wake-lock=(:YQ==:)",
        ] {
            let mut headers = HeaderList::new();
            headers.append("permissions-policy", value).unwrap();
            assert!(!screen_wake_lock_allowed(&headers), "{value}");
        }
        let mut headers = HeaderList::new();
        headers
            .append("permissions-policy", "screen-wake-lock=(42 self)")
            .unwrap();
        assert!(screen_wake_lock_allowed(&headers));
    }

    #[test]
    fn malformed_dictionary_fails_closed() {
        for value in [
            "screen-wake-lock=(self",
            "screen-wake-lock=(),",
            "screen-wake-lock = (self)",
            "other=(\"unfinished), screen-wake-lock=self",
        ] {
            let mut headers = HeaderList::new();
            headers.append("permissions-policy", value).unwrap();
            assert!(!screen_wake_lock_allowed(&headers), "{value}");
        }
    }
}
