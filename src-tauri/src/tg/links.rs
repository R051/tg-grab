//! t.me link parsing and synthesis.

use regex::Regex;

/// Reference to a channel/supergroup, either by public username or raw channel id.
#[derive(Debug, Clone)]
pub enum ChatLink {
    Username(String),
    ChannelRaw(i64),
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

/// Parse a channel/group reference: `@name`, `name`, `https://t.me/name`, `t.me/c/123456`.
/// Invite links (`t.me/+hash`) are rejected with a human-readable error.
pub fn parse_chat_link(input: &str) -> Result<ChatLink, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("INVALID_LINK".into());
    }
    if re(r"(?:t\.me|telegram\.me)/\+")
        .is_match(s)
        .then_some(())
        .is_some()
    {
        return Err("INVITE_LINK".into());
    }
    let s = re(r"^https?://").replace(s, "");
    let s = re(r"^(?:t\.me|telegram\.me)/").replace(&s, "");

    if let Some(m) = re(r"^c/(\d+)").captures(&s) {
        return Ok(ChatLink::ChannelRaw(m[1].parse().map_err(|_| "INVALID_LINK")?));
    }
    let username = s
        .strip_prefix('@')
        .unwrap_or(&s)
        .split('/')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if username.is_empty() || !re(r"^[A-Za-z0-9_]{3,64}$").is_match(&username) {
        return Err("INVALID_LINK".into());
    }
    Ok(ChatLink::Username(username))
}

/// Parse a message link: `https://t.me/name/123` or `https://t.me/c/123456/789`.
/// Returns the chat reference and the message id.
pub fn parse_msg_link(input: &str) -> Result<(ChatLink, i32), String> {
    let s = input.trim();
    let url = s.split('?').next().unwrap_or(s);

    if let Some(m) = re(r"(?:https?://)?(?:t\.me|telegram\.me)/c/(\d+)/(\d+)")
        .captures(url)
    {
        return Ok((
            ChatLink::ChannelRaw(m[1].parse().map_err(|_| "INVALID_LINK")?),
            m[2].parse().map_err(|_| "INVALID_LINK")?,
        ));
    }
    let m = re(r"(?:https?://)?(?:t\.me|telegram\.me)/([^/\s]+)/(\d+)")
        .captures(url)
        .ok_or("INVALID_LINK")?;
    let entity = m[1].to_string();
    if entity.lstrip_digits_is_int() {
        // Bare numeric entity: treat as a private channel id (same as t.me/c/<id>/...).
        return Ok((
            ChatLink::ChannelRaw(entity.parse().map_err(|_| "INVALID_LINK")?),
            m[2].parse().map_err(|_| "INVALID_LINK")?,
        ));
    }
    Ok((ChatLink::Username(entity), m[2].parse().map_err(|_| "INVALID_LINK")?))
}

trait IntCheck {
    fn lstrip_digits_is_int(&self) -> bool;
}
impl IntCheck for str {
    fn lstrip_digits_is_int(&self) -> bool {
        let t = self.trim_start_matches('-');
        !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
    }
}

/// Build a public message link used for queue display and later re-parsing.
pub fn make_msg_link(username: Option<&str>, raw_id: i64, msg_id: i32) -> String {
    match username.filter(|u| !u.is_empty()) {
        Some(u) => format!("https://t.me/{u}/{msg_id}"),
        None => format!("https://t.me/c/{raw_id}/{msg_id}"),
    }
}

/// Strip characters that are illegal in Windows filenames, cap the length.
pub fn sanitize_filename(name: &str) -> String {
    let bad = Regex::new(r#"[<>:"/\\|?*]"#).expect("valid regex");
    let mut name = bad.replace_all(name, "_").to_string();
    if name.chars().count() > 200 {
        // keep the extension when truncating
        let dot = name.rfind('.');
        match dot {
            Some(idx) if name.len() - idx <= 10 => {
                let ext = name[idx..].to_string();
                let base: String = name.chars().take(196).collect();
                name = format!("{base}{ext}");
            }
            _ => {
                name = name.chars().take(200).collect();
            }
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usernames() {
        assert!(matches!(
            parse_chat_link("https://t.me/duolaxiazi1"),
            Ok(ChatLink::Username(u)) if u == "duolaxiazi1"
        ));
        assert!(matches!(
            parse_chat_link("@duo"),
            Ok(ChatLink::Username(u)) if u == "duo"
        ));
        assert!(matches!(
            parse_msg_link("https://t.me/telegram/459"),
            Ok((ChatLink::Username(_), 459))
        ));
        assert!(matches!(
            parse_msg_link("https://t.me/c/2489506583/1350?x=1"),
            Ok((ChatLink::ChannelRaw(2489506583), 1350))
        ));
        assert_eq!(
            parse_chat_link("https://t.me/+AbCdEf").unwrap_err(),
            "INVITE_LINK"
        );
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_filename("a<b>:c?.mp4"), "a_b__c_.mp4");
    }
}
