use fancy_regex::Regex;
use rand::{seq::SliceRandom, thread_rng, Rng};

pub const DEFAULT_REGEX: &str = r"^(?=.{8,128}$)(?=.*[a-z])(?=.*[A-Z])(?=.*[^A-Za-z0-9\s]).*$";
pub const DEFAULT_HINT: &str =
    "密码必须为 8–128 位，并至少包含一个大写字母、一个小写字母和一个特殊字符";

const DEFAULT_GENERATED_LENGTH: usize = 8;
const MIN_GENERATED_LENGTH: usize = 8;
const MAX_GENERATED_LENGTH: usize = 128;
const GENERATION_ATTEMPTS: usize = 1024;

pub struct PasswordPolicy {
    regex: Regex,
    hint: String,
    generated_length: usize,
}

impl PasswordPolicy {
    pub fn from_env() -> Result<Self, String> {
        let pattern = std::env::var("CANGLING_TILE_PASSWORD_REGEX")
            .unwrap_or_else(|_| DEFAULT_REGEX.to_owned());
        let hint = std::env::var("CANGLING_TILE_PASSWORD_HINT")
            .unwrap_or_else(|_| DEFAULT_HINT.to_owned());
        let generated_length = match std::env::var("CANGLING_TILE_PASSWORD_GENERATED_LENGTH") {
            Ok(value) => value
                .parse::<usize>()
                .map_err(|_| "CANGLING_TILE_PASSWORD_GENERATED_LENGTH 必须是整数".to_owned())?,
            Err(_) => DEFAULT_GENERATED_LENGTH,
        };
        Self::new(&pattern, hint, generated_length)
    }

    fn new(pattern: &str, hint: String, generated_length: usize) -> Result<Self, String> {
        if !(MIN_GENERATED_LENGTH..=MAX_GENERATED_LENGTH).contains(&generated_length) {
            return Err(format!(
                "CANGLING_TILE_PASSWORD_GENERATED_LENGTH 必须在 {MIN_GENERATED_LENGTH}–{MAX_GENERATED_LENGTH} 之间"
            ));
        }
        let regex = Regex::new(pattern)
            .map_err(|error| format!("CANGLING_TILE_PASSWORD_REGEX 无效：{error}"))?;
        Ok(Self {
            regex,
            hint,
            generated_length,
        })
    }

    pub fn validate(&self, password: &str) -> Result<(), String> {
        match self.regex.is_match(password) {
            Ok(true) => Ok(()),
            Ok(false) => Err(self.hint.clone()),
            Err(error) => Err(format!("密码规则匹配失败：{error}")),
        }
    }

    pub fn generate(&self) -> Result<String, String> {
        const LOWER: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
        const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
        const DIGITS: &[u8] = b"23456789";
        const SPECIAL: &[u8] = b"!@#$%^&*_-+=";

        let mut rng = thread_rng();
        let all = [LOWER, UPPER, DIGITS, SPECIAL].concat();
        for _ in 0..GENERATION_ATTEMPTS {
            let mut bytes = vec![
                *LOWER.choose(&mut rng).unwrap(),
                *UPPER.choose(&mut rng).unwrap(),
                *DIGITS.choose(&mut rng).unwrap(),
                *SPECIAL.choose(&mut rng).unwrap(),
            ];
            bytes.extend(
                (bytes.len()..self.generated_length).map(|_| all[rng.gen_range(0..all.len())]),
            );
            bytes.shuffle(&mut rng);
            let password = String::from_utf8(bytes).expect("password alphabet must be ASCII");
            if self.validate(&password).is_ok() {
                return Ok(password);
            }
        }
        Err("无法生成满足密码规则的随机密码；请通过 -p 显式指定密码，或调整 CANGLING_TILE_PASSWORD_REGEX".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_policy(length: usize) -> PasswordPolicy {
        PasswordPolicy::new(DEFAULT_REGEX, DEFAULT_HINT.to_owned(), length).unwrap()
    }

    #[test]
    fn default_policy_requires_length_case_and_special_character() {
        let policy = default_policy(8);
        assert!(policy.validate("Aa-short!").is_ok());
        assert!(policy.validate("aa-short!").is_err());
        assert!(policy.validate("AA-SHORT!").is_err());
        assert!(policy.validate("AaShort12").is_err());
        assert!(policy.validate("Aa-shr!").is_err());
    }

    #[test]
    fn generated_password_matches_default_policy() {
        for length in [8, 24] {
            let policy = default_policy(length);
            let password = policy.generate().unwrap();
            assert_eq!(password.len(), length);
            assert!(policy.validate(&password).is_ok());
        }
    }

    #[test]
    fn custom_policy_is_enforced() {
        let policy = PasswordPolicy::new(
            r"^CIS-[A-Z]{4}$",
            "密码必须采用 CIS-XXXX 格式".to_owned(),
            8,
        )
        .unwrap();
        assert!(policy.validate("CIS-ABCD").is_ok());
        assert_eq!(
            policy.validate("Aa-short!").unwrap_err(),
            "密码必须采用 CIS-XXXX 格式"
        );
    }
}
