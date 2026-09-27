use std::fmt;

/// Declares a string-backed enum with `as_str`, `ALL`, `Display` and `FromStr`.
///
/// Domain enums stay free of serde; the API layer (de)serializes them via `Display`/`FromStr`,
/// the database stores `as_str()`.
macro_rules! str_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name { $($variant),+ }

        impl $name {
            #[allow(dead_code)]
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::shared::kernel::ParseEnumError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($text => Ok($name::$variant),)+
                    _ => Err($crate::shared::kernel::ParseEnumError {
                        value: s.to_owned(),
                        expected: &[$($text),+],
                    }),
                }
            }
        }
    };
}
pub(crate) use str_enum;

/// Returned when a string is not a valid enum value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEnumError {
    pub value: String,
    pub expected: &'static [&'static str],
}

impl fmt::Display for ParseEnumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid value `{}`, expected one of: {}",
            self.value,
            self.expected.join(", ")
        )
    }
}

impl std::error::Error for ParseEnumError {}

#[cfg(test)]
mod tests {
    str_enum! {
        enum Color { Red = "red", DarkBlue = "dark_blue" }
    }

    #[test]
    fn round_trips_and_rejects_unknown_values() {
        assert_eq!("dark_blue".parse::<Color>(), Ok(Color::DarkBlue));
        assert_eq!(Color::Red.to_string(), "red");
        let err = "green".parse::<Color>().unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid value `green`, expected one of: red, dark_blue"
        );
    }
}
