use std::fmt;

macro_rules! tmux_id {
    ($name:ident, $prefix:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// `raw` must already include the tmux prefix (e.g. `"$3"`, `"@12"`, `"%7"`).
            pub fn new(raw: impl Into<String>) -> Self {
                let raw = raw.into();
                debug_assert!(
                    raw.starts_with($prefix),
                    "{} id must start with '{}', got {:?}",
                    stringify!($name),
                    $prefix,
                    raw
                );
                Self(raw)
            }

            /// The tmux target string for this id, e.g. `$3`, suitable for `-t` arguments.
            pub fn as_target(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

tmux_id!(SessionId, "$");
tmux_id!(WindowId, "@");
tmux_id!(PaneId, "%");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_target_returns_raw_id() {
        let s = SessionId::new("$3");
        assert_eq!(s.as_target(), "$3");
        assert_eq!(s.to_string(), "$3");
    }

    #[test]
    fn window_and_pane_ids_round_trip() {
        assert_eq!(WindowId::new("@12").as_target(), "@12");
        assert_eq!(PaneId::new("%7").as_target(), "%7");
    }
}
