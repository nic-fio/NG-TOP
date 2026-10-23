// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Destinazione ssh: `host`, `user@host`, `ssh://[user@]host[:porta]`.

/// Macchina da monitorare. I valori sono validati: niente `-` iniziale (iniezione di opzioni ssh),
/// niente spazi o caratteri di controllo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    user: Option<String>,
    host: String,
    port: Option<u16>,
}

fn valid_user(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '\\'))
}

fn valid_host(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '%'))
}

impl Target {
    pub fn parse(s: &str) -> Result<Target, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty host".into());
        }
        if s.starts_with('-') {
            return Err(format!("invalid host \"{s}\" (must not start with '-')"));
        }
        let (rest, url) = match s.strip_prefix("ssh://") {
            Some(r) => (r.trim_end_matches('/'), true),
            None => (s, false),
        };
        let (user, hostport) = match rest.rsplit_once('@') {
            Some((u, h)) => (Some(u), h),
            None => (None, rest),
        };
        if let Some(u) = user
            && !valid_user(u)
        {
            return Err(format!("invalid user name \"{u}\""));
        }
        // porta: solo con ssh://; l'host può essere un IPv6 tra parentesi quadre
        let (host, port) = if let Some(r) = hostport.strip_prefix('[') {
            let (h, after) = r.split_once(']').ok_or("missing ']' in host")?;
            let port = match after {
                "" => None,
                a => Some(a.strip_prefix(':').ok_or("invalid host")?),
            };
            (h, port)
        } else if url && hostport.matches(':').count() == 1 {
            let (h, p) = hostport.split_once(':').unwrap();
            (h, Some(p))
        } else if !url && hostport.matches(':').count() == 1 {
            return Err(format!(
                "invalid host \"{hostport}\" (use ssh://host:port to give a port)"
            ));
        } else {
            (hostport, None)
        };
        if !valid_host(host) {
            return Err(format!("invalid host \"{host}\""));
        }
        let port = match port {
            None => None,
            Some(p) => match p.parse::<u16>() {
                Ok(n) if n > 0 => Some(n),
                _ => return Err(format!("invalid port \"{p}\"")),
            },
        };
        Ok(Target {
            user: user.map(str::to_string),
            host: host.to_string(),
            port,
        })
    }

    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// Testo per l'utente: `user@host` o `user@host:porta`.
    pub fn display(&self) -> String {
        let mut s = String::new();
        if let Some(u) = &self.user {
            s.push_str(u);
            s.push('@');
        }
        if self.host.contains(':') {
            s.push_str(&format!("[{}]", self.host));
        } else {
            s.push_str(&self.host);
        }
        if let Some(p) = self.port {
            s.push_str(&format!(":{p}"));
        }
        s
    }

    /// Argomento `[user@]host` per ssh.
    pub(crate) fn ssh_dest(&self) -> String {
        match &self.user {
            Some(u) => format!("{u}@{}", self.host),
            None => self.host.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forme_valide() {
        let t = Target::parse("host").unwrap();
        assert_eq!((t.user(), t.host(), t.port()), (None, "host", None));
        let t = Target::parse("al@h.example.com").unwrap();
        assert_eq!(t.display(), "al@h.example.com");
        let t = Target::parse("ssh://al@h:2222").unwrap();
        assert_eq!(
            (t.user(), t.host(), t.port()),
            (Some("al"), "h", Some(2222))
        );
        assert_eq!(t.display(), "al@h:2222");
        assert_eq!(Target::parse("ssh://host").unwrap().display(), "host");
        let t = Target::parse("ssh://u@[::1]:22").unwrap();
        assert_eq!((t.host(), t.port()), ("::1", Some(22)));
        assert_eq!(Target::parse("fe80::1").unwrap().host(), "fe80::1");
    }

    #[test]
    fn forme_rifiutate() {
        for s in [
            "",
            "-oProxyCommand=x",
            "u@-h",
            "-u@h",
            "ssh://-h",
            "a b",
            "h;ls",
            "h`x`",
            "h$(x)",
            "u@h:22",
            "ssh://h:0",
            "ssh://h:99999",
            "ssh://h:ab",
            "u v@h",
            "h\nx",
            "@h",
            "u@",
        ] {
            assert!(Target::parse(s).is_err(), "{s:?} doveva essere rifiutato");
        }
    }
}
