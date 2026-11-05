use std::collections::BTreeMap;

/// Templates contain literal text and named values only. Values are never evaluated.
pub fn fields(text: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        if rest[..start].contains("}}") {
            return Err("템플릿 괄호를 확인해 주세요".into());
        }
        rest = &rest[start + 2..];
        let end = rest.find("}}").ok_or("템플릿의 닫는 괄호가 필요해요")?;
        let name = rest[..end].trim();
        if name.is_empty() || name.chars().count() > 40 || name.contains(['{', '}', '\n', '\r']) {
            return Err("빈칸 이름은 괄호와 줄바꿈 없이 1~40자로 입력해 주세요".into());
        }
        if !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
        if names.len() > 32 {
            return Err("빈칸은 최대 32개까지 사용할 수 있어요".into());
        }
        rest = &rest[end + 2..];
    }
    if rest.contains("}}") {
        return Err("템플릿 괄호를 확인해 주세요".into());
    }
    Ok(names)
}
pub fn render(text: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let names = fields(text)?;
    if values.len() != names.len() || names.iter().any(|n| !values.contains_key(n)) {
        return Err("모든 빈칸을 입력해 주세요".into());
    }
    let mut result = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let end = rest[start + 2..].find("}}").unwrap() + start + 2;
        result.push_str(&values[rest[start + 2..end].trim()]);
        rest = &rest[end + 2..];
        if result.encode_utf16().count() > 16_384 {
            return Err("완성 문구는 16,384자 이내로 입력해 주세요".into());
        }
    }
    result.push_str(rest);
    if result.contains('\0') || result.encode_utf16().count() > 16_384 {
        return Err("완성 문구를 확인해 주세요".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_fields_are_literal_and_bounded() {
        let values = BTreeMap::from([("고객명".into(), "{{날짜}}".into())]);
        assert_eq!(fields("{{고객명}} / {{ 고객명 }}").unwrap(), vec!["고객명"]);
        assert_eq!(
            render("{{고객명}} / {{고객명}}", &values).unwrap(),
            "{{날짜}} / {{날짜}}"
        );
        assert!(fields("{{미완성").is_err());
        assert!(render("{{고객명}}", &BTreeMap::new()).is_err());
        assert!(render(
            "{{고객명}}",
            &BTreeMap::from([("고객명".into(), "😀".repeat(8193))])
        )
        .is_err());
    }
}
