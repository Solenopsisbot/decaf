use crate::runtime::host_ref::{Methods, instance_of_class};
use crate::runtime::{MethodRef, NativeError, Value};
pub(crate) const URL_METHODS: Methods = Methods {
    to_string: |h, _| Ok(Some(h.key().to_owned())),
    hash_code: |_, _| Ok(None),
    equals: |_, _, _| Ok(None),
    instance_of: instance_of_class,
    crosses_jars: |_, _| true,
    compare_to: None,
};
#[derive(Debug, PartialEq)]
pub enum UrlError {
    Relative,
    UnknownProtocol,
    Jar,
}
fn scheme(uri: &str) -> Option<&str> {
    let (s, _rest) = uri.split_once(':')?;
    if s.is_empty()
        || !s.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_alphabetic()
                || (i > 0 && (b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.')))
        })
    {
        None
    } else {
        Some(s)
    }
}
pub fn to_url_text(uri: &str) -> Result<String, UrlError> {
    let raw = scheme(uri).ok_or(UrlError::Relative)?;
    let s = raw.to_ascii_lowercase();
    if !matches!(
        s.as_str(),
        "http" | "https" | "file" | "jar" | "ftp" | "mailto" | "jrt" | "jmod"
    ) {
        return Err(UrlError::UnknownProtocol);
    };
    if s == "jar" && !uri.contains("!/") {
        return Err(UrlError::Jar);
    }
    let tail = &uri[raw.len()..];
    if tail.starts_with(":///") && s != "jar" {
        return Ok(format!("{s}:/{}", &tail[4..]));
    }
    Ok(format!("{s}{tail}"))
}
pub fn instance_call(r: &MethodRef, receiver: &Value) -> Option<Result<Value, NativeError>> {
    if r.owner == "java/net/URL" {
        if let Value::Host(h) = receiver
            && h.kind() == crate::runtime::host_ref::HostKind::Url
        {
            if r.name == "toExternalForm" || r.name == "toString" {
                return Some(Ok(Value::Ref(Some(h.key().to_owned()))));
            }
            if r.name == "getProtocol" {
                return Some(Ok(Value::Ref(Some(
                    h.key().split(':').next().unwrap_or("").to_owned(),
                ))));
            }
        }
        return None;
    }
    if r.owner != "java/net/URI" || r.name != "toURL" {
        return None;
    }
    let text = crate::runtime::host_jdk_value::uri_text(receiver)?;
    Some(match to_url_text(text) {
        Ok(t) => Ok(Value::Host(crate::runtime::host_ref::HostRef::new(
            crate::runtime::host_ref::HostKind::Url,
            t,
        ))),
        Err(UrlError::Relative) => Err(NativeError::UncaughtException {
            class: "java/lang/IllegalArgumentException".into(),
            message: Some("URI is not absolute".into()),
            frame: None,
        }),
        Err(UrlError::Jar) => Err(NativeError::UncaughtException {
            class: "java/net/MalformedURLException".into(),
            message: Some("no !/ in spec".into()),
            frame: None,
        }),
        Err(UrlError::UnknownProtocol) => Err(NativeError::UncaughtException {
            class: "java/net/MalformedURLException".into(),
            // Java looks the protocol up lowercased and names it so (`DATA:x` gives `data`).
            message: Some(format!(
                "unknown protocol: {}",
                text.split(':').next().unwrap_or("").to_ascii_lowercase()
            )),
            frame: None,
        }),
    })
}
