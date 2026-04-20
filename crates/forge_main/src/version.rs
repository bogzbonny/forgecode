/// Application version information.
pub const VERSION: &str = match option_env!("APP_VERSION") {
    None => env!("CARGO_PKG_VERSION"),
    Some(v) => v,
};
