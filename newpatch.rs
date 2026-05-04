    for cookie_part in raw_cookie_header.split(';') {
        let trimmed = cookie_part.trim();

        if let Some((name, value)) = trimmed.split_once('=') {
            if name == cookie_name {
                return Some(value.to_string());
            }
        }
    }
