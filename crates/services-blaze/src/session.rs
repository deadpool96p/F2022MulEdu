//! Player connection session state tracking.

#[derive(Debug, Clone)]
pub struct SessionState {
    pub user_id: u64,
    pub persona_id: u64,
    pub display_name: String,
    pub session_key: String,
    pub external_ip: String,
    pub internal_ip: String,
    pub port: u16,
    pub nat_type: u8,
    pub is_logged_in: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            user_id: 0,
            persona_id: 0,
            display_name: "Guest".to_string(),
            session_key: String::new(),
            external_ip: "127.0.0.1".to_string(),
            internal_ip: "127.0.0.1".to_string(),
            port: 3659,
            nat_type: 0, // 0 = OPEN NAT
            is_logged_in: false,
        }
    }
}
