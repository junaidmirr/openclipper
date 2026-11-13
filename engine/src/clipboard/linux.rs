pub mod linux_files {
    /// On Linux, file copy typically puts URIs in format file:///path/to/file in text/uri-list
    pub fn parse_uri_list(text: &str) -> Option<Vec<String>> {
        let mut files = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if let Some(stripped) = trimmed.strip_prefix("file://") {
                // Decode percent-encoded paths
                let path = percent_decode(stripped);
                if std::path::Path::new(&path).exists() {
                    files.push(path);
                }
            } else if std::path::Path::new(trimmed).is_absolute() && std::path::Path::new(trimmed).exists() {
                files.push(trimmed.to_string());
            }
        }

        if files.is_empty() {
            None
        } else {
            Some(files)
        }
    }

    fn percent_decode(input: &str) -> String {
        let mut bytes = Vec::new();
        let mut chars = input.bytes();
        while let Some(b) = chars.next() {
            if b == b'%' {
                if let (Some(h1), Some(h2)) = (chars.next(), chars.next()) {
                    if let Ok(val) = u8::from_str_radix(
                        &format!("{}{}", h1 as char, h2 as char),
                        16,
                    ) {
                        bytes.push(val);
                        continue;
                    }
                }
            }
            bytes.push(b);
        }
        String::from_utf8_lossy(&bytes).to_string()
    }
}
