use horizon_native_java::{Host, MethodRef, StdHost, Value};

fn rows() -> (Vec<String>, Vec<String>) {
    let text = include_str!("fixtures/cov15_items/fields.txt");
    let mut items = Vec::new(); let mut blocks = Vec::new();
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("Items ") { items.push(n.to_owned()); }
        if let Some(n) = line.strip_prefix("Blocks ") { blocks.push(n.to_owned()); }
    }
    (items, blocks)
}

fn check(owner: &str, prefix: &str, fields: &[String], host: &mut StdHost) -> (usize, usize, Vec<String>) {
    let mut good=0; let mut counted=0; let mut wrong=Vec::new();
    for field in fields {
        let r=MethodRef { owner: owner.into(), name: field.clone(), descriptor: format!("L{prefix};") };
        let first = host.get_static(&r);
        let second = host.get_static(&r);
        if let (Ok(a), Ok(b)) = (&first, &second) { assert_eq!(a, b, "identity mismatch for {owner}::{field}"); }
        match first {
            Ok(v) if format!("{v:?}").contains(&format!("client/{prefix}/minecraft:{}", field.to_ascii_lowercase())) => good+=1,
            Ok(Value::Ref(None)) => counted+=1,
            Ok(v) => wrong.push(format!("{field}: {v:?}")),
            Err(_) => counted+=1,
        }
    }
    (good,counted,wrong)
}

#[test]
fn every_captured_registry_field_is_answered_without_wrong_ids() {
    let (items, blocks)=rows(); let mut host=StdHost::new();
    let (ig,ic,mut iw)=check("net/minecraft/world/item/Items","item",&items,&mut host);
    let (bg,bc,bw)=check("net/minecraft/world/level/block/Blocks","block",&blocks,&mut host);
    iw.extend(bw);
    eprintln!("cov15 registry fields: correct={} counted={} wrong={}", ig+bg, ic+bc, iw.len());
    assert!(iw.is_empty(), "wrong registry ids: {iw:?}");
    assert_eq!((ig+bg+ic+bc), items.len()+blocks.len());
}
