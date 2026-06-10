use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub stub_status_url: String,
    pub access_log_path: String,
    pub domain_auth: String,
    pub memory_cap_bytes: usize,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()
                .expect("PORT must be a number"),
            stub_status_url: env::var("STUB_STATUS_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080/nginx_status".to_string()),
            access_log_path: env::var("ACCESS_LOG_PATH")
                .unwrap_or_else(|_| "/var/log/nginx/access.log".to_string()),
            domain_auth: env::var("DOMAIN_AUTH").expect("DOMAIN_AUTH is required"),
            // default 70MB
            memory_cap_bytes: env::var("MEMORY_CAP_MB")
                .unwrap_or_else(|_| "70".to_string())
                .parse::<usize>()
                .expect("MEMORY_CAP_MB must be a number")
                * 1024
                * 1024,
        }
    }
}
