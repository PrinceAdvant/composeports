//! List host ports published by Docker Compose files and detect conflicts.
//!
//! Running several apps on one server usually means several Compose files.
//! If two of them publish the same host port, the second `docker compose up`
//! fails with "port is already allocated". `composeports` reads the files,
//! expands the short and long `ports` syntax and reports every binding and
//! every clash.
//!
//! ```
//! let yaml = r#"
//! services:
//!   web:
//!     ports: ["8080:80", "127.0.0.1:5432:5432"]
//!   admin:
//!     ports:
//!       - target: 80
//!         published: 8080
//! "#;
//! let b = composeports::parse(yaml, "app.yml").unwrap();
//! assert_eq!(b.len(), 3);
//! let c = composeports::conflicts(&b);
//! assert_eq!(c.len(), 1);
//! assert_eq!(c[0].0.host_port, 8080);
//! ```
//!
//! Publishing app ports on the host is often unnecessary when a reverse
//! proxy routes by hostname. See the guide on
//! [how to run multiple apps on one VPS](https://peon.sh/blogs/how-to-run-multiple-apps-on-one-vps)
//! for that layout. Built by a contributor to [Peon](https://peon.sh/), an
//! open-source self-hosted deployment platform.

use serde_yaml::Value;

/// One published port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// File the binding came from.
    pub file: String,
    /// Compose service name.
    pub service: String,
    /// Host IP, or empty for all interfaces.
    pub host_ip: String,
    /// Host port.
    pub host_port: u16,
    /// Container port.
    pub container_port: u16,
    /// "tcp" or "udp".
    pub protocol: String,
}

impl Binding {
    fn all_interfaces(&self) -> bool {
        self.host_ip.is_empty() || self.host_ip == "0.0.0.0" || self.host_ip == "::"
    }
    /// True when two bindings cannot both be bound on one host.
    pub fn clashes(&self, other: &Binding) -> bool {
        self.host_port == other.host_port
            && self.protocol == other.protocol
            && (self.all_interfaces() || other.all_interfaces() || self.host_ip == other.host_ip)
    }
}

/// Error from parsing a Compose file.
#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}

fn range(s: &str) -> Result<Vec<u16>, Error> {
    let bad = || Error(format!("invalid port {s:?}"));
    match s.split_once('-') {
        Some((a, b)) => {
            let (a, b): (u16, u16) = (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?);
            if a > b {
                return Err(bad());
            }
            Ok((a..=b).collect())
        }
        None => Ok(vec![s.parse().map_err(|_| bad())?]),
    }
}

/// Parse a short syntax entry such as `"127.0.0.1:8080:80/tcp"`.
/// Entries with only a container port publish a random host port and are
/// skipped.
pub fn parse_short(spec: &str) -> Result<Vec<(String, u16, u16, String)>, Error> {
    let (spec, proto) = match spec.rsplit_once('/') {
        Some((s, p)) => (s, p.to_string()),
        None => (spec, "tcp".to_string()),
    };
    // IPv6 host IPs are written in brackets: "[::1]:8080:80".
    let (ip, rest) = if let Some(r) = spec.strip_prefix('[') {
        let (ip, rest) = r.split_once("]:").ok_or_else(|| Error(format!("invalid port {spec:?}")))?;
        (ip.to_string(), rest.to_string())
    } else {
        let parts: Vec<&str> = spec.split(':').collect();
        match parts.len() {
            1 => return Ok(vec![]),
            2 => (String::new(), spec.to_string()),
            3 => (parts[0].to_string(), format!("{}:{}", parts[1], parts[2])),
            _ => return Err(Error(format!("invalid port {spec:?}"))),
        }
    };
    let (h, c) = rest.split_once(':').ok_or_else(|| Error(format!("invalid port {spec:?}")))?;
    if h.is_empty() {
        return Ok(vec![]);
    }
    let hs = range(h)?;
    let cs = range(c)?;
    let pair = |i: usize| if cs.len() == hs.len() { cs[i] } else { cs[0] };
    Ok(hs.iter().enumerate().map(|(i, hp)| (ip.clone(), *hp, pair(i), proto.clone())).collect())
}

fn as_port(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Parse Compose YAML and return its published host ports.
pub fn parse(yaml: &str, file: &str) -> Result<Vec<Binding>, Error> {
    let doc: Value = serde_yaml::from_str(yaml).map_err(|e| Error(format!("{file}: {e}")))?;
    let mut out = Vec::new();
    let Some(services) = doc.get("services").and_then(Value::as_mapping) else {
        return Ok(out);
    };
    for (name, svc) in services {
        let service = name.as_str().unwrap_or_default().to_string();
        let Some(ports) = svc.get("ports").and_then(Value::as_sequence) else { continue };
        for p in ports {
            let entries = match p {
                Value::Mapping(m) => {
                    let get = |k: &str| m.get(Value::from(k));
                    let Some(published) = get("published").and_then(as_port) else { continue };
                    let target = get("target").and_then(as_port).unwrap_or_default();
                    let ip = get("host_ip").and_then(Value::as_str).unwrap_or("").to_string();
                    let proto = get("protocol").and_then(Value::as_str).unwrap_or("tcp").to_string();
                    let hs = range(&published)?;
                    let c: u16 = target.parse().map_err(|_| Error(format!("{file}: invalid target {target:?}")))?;
                    hs.into_iter().map(|h| (ip.clone(), h, c, proto.clone())).collect()
                }
                other => {
                    let s = as_port(other).ok_or_else(|| Error(format!("{file}: unsupported port entry")))?;
                    parse_short(&s).map_err(|e| Error(format!("{file}: {e}")))?
                }
            };
            for (host_ip, host_port, container_port, protocol) in entries {
                out.push(Binding { file: file.to_string(), service: service.clone(), host_ip, host_port, container_port, protocol });
            }
        }
    }
    Ok(out)
}

/// Return every pair of bindings that cannot coexist on one host.
pub fn conflicts(bindings: &[Binding]) -> Vec<(Binding, Binding)> {
    let mut out = Vec::new();
    for (i, a) in bindings.iter().enumerate() {
        for b in &bindings[i + 1..] {
            if a.clashes(b) {
                out.push((a.clone(), b.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_forms() {
        assert_eq!(parse_short("80").unwrap(), vec![]);
        assert_eq!(parse_short("8080:80").unwrap(), vec![(String::new(), 8080, 80, "tcp".into())]);
        assert_eq!(parse_short("127.0.0.1:5432:5432").unwrap()[0].0, "127.0.0.1");
        assert_eq!(parse_short("53:53/udp").unwrap()[0].3, "udp");
        assert_eq!(parse_short("[::1]:6379:6379").unwrap()[0].0, "::1");
        assert_eq!(parse_short("9000-9002:9000-9002").unwrap().len(), 3);
        assert_eq!(parse_short("127.0.0.1::80").unwrap(), vec![]);
        assert!(parse_short("abc:80").is_err());
    }

    #[test]
    fn localhost_and_all_interfaces() {
        let y = "services:\n  a:\n    ports: ['127.0.0.1:8080:80']\n  b:\n    ports: ['127.0.0.2:8080:80']\n  c:\n    ports: ['8081:80', '8081:81/udp']\n";
        let b = parse(y, "x.yml").unwrap();
        assert_eq!(b.len(), 4);
        assert!(conflicts(&b).is_empty());
        let y2 = "services:\n  d:\n    ports: ['8080:3000']\n";
        let mut all = b.clone();
        all.extend(parse(y2, "y.yml").unwrap());
        assert_eq!(conflicts(&all).len(), 2);
    }

    #[test]
    fn no_services() {
        assert!(parse("version: '3'\n", "z.yml").unwrap().is_empty());
    }
}
