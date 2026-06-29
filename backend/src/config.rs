pub const DEFAULT_API_ADDR: &str = "127.0.0.1:5174";

pub fn api_addr_from_env() -> String {
    api_addr(
        std::env::var("STUDY_SCHEDULER_API_ADDR").ok().as_deref(),
        std::env::var("STUDY_SCHEDULER_API_PORT").ok().as_deref(),
    )
}

fn api_addr(configured_addr: Option<&str>, configured_port: Option<&str>) -> String {
    if let Some(addr) = non_empty(configured_addr) {
        return addr.to_string();
    }

    if let Some(port) = non_empty(configured_port) {
        return format!("127.0.0.1:{port}");
    }

    DEFAULT_API_ADDR.to_string()
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_API_ADDR, api_addr};

    #[test]
    fn defaults_to_localhost_api_addr() {
        assert_eq!(api_addr(None, None), DEFAULT_API_ADDR);
    }

    #[test]
    fn uses_explicit_api_addr_before_port() {
        assert_eq!(
            api_addr(Some("127.0.0.1:6000"), Some("7000")),
            "127.0.0.1:6000",
        );
    }

    #[test]
    fn derives_localhost_addr_from_api_port() {
        assert_eq!(api_addr(None, Some("6000")), "127.0.0.1:6000");
    }

    #[test]
    fn ignores_blank_env_values() {
        assert_eq!(api_addr(Some(" "), Some("")), DEFAULT_API_ADDR);
    }
}
