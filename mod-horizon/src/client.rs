//! Client-side resident jar loading and registration inventory.
//!
//! This phase owns one interpreter host per jar on the client. It runs client entrypoints and
//! NeoForge's client loading stages, then records registrations and listeners for later G1 hooks.
//! It deliberately does not draw or dispatch Java client callbacks yet; the owner hooks only keep
//! the per-jar hosts and the report inventory alive across the client lifecycle.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{JAR_BUDGET, LOG, caught, jar_id};
use runtime::events::{GameListener, PhaseOrdering};
use runtime::{ExecutionBudget, NativeJar, StdHost};

use crate::runtime;

#[derive(Default)]
pub(crate) struct State {
    loaded: bool,
    world_loaded: bool,
    jars: Vec<LoadedJar>,
    refused: Vec<Row>,
    retired: Vec<Row>,
    #[cfg(test)]
    fixture_files: Option<Vec<(String, Vec<u8>)>>,
}

struct LoadedJar {
    row: Row,
    jar: Arc<NativeJar>,
    host: StdHost,
    listeners: Vec<GameListener>,
    orderings: Vec<PhaseOrdering>,
}

#[derive(Clone, Debug, Default)]
struct StageCounts {
    attempted: usize,
    failed: usize,
    disabled: usize,
    calls: usize,
}

#[derive(Clone, Debug, Default)]
struct Row {
    file: String,
    id: String,
    verdict: &'static str,
    host_calls: usize,
    unknown_host_calls: usize,
    reason: Option<String>,
    entrypoints_ran: usize,
    entrypoints_failed: usize,
    stages: BTreeMap<String, StageCounts>,
    keys: usize,
    hud_layers: usize,
    listeners: usize,
    unsupported: BTreeMap<String, usize>,
    refused_registrations: Vec<String>,
}

impl Row {
    fn refused(file: &str, id: String, reason: impl Into<String>) -> Self {
        Self {
            file: file.to_owned(),
            id,
            verdict: "refused",
            reason: Some(reason.into()),
            ..Self::default()
        }
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "file": self.file,
            "id": self.id,
            "verdict": self.verdict,
            "host_calls": self.host_calls,
            "unknown_host_call_count": self.unknown_host_calls,
            "reason": self.reason,
            "client": {
                "entrypoints_ran": self.entrypoints_ran,
                "entrypoints_failed": self.entrypoints_failed,
                "stages": self.stages.iter().map(|(stage, counts)| (stage.clone(), serde_json::json!({
                    "attempted": counts.attempted,
                    "failed": counts.failed,
                    "disabled": counts.disabled,
                    "calls": counts.calls,
                }))).collect::<serde_json::Map<_, _>>(),
                "client_keys": self.keys,
                "client_hud_layers": self.hud_layers,
                "client_listeners": self.listeners,
                "unfired_registrations": self.unsupported.iter().map(|(source, count)| serde_json::json!({"source": source, "count": count})).collect::<Vec<_>>(),
                "refused": self.refused_registrations,
            }
        })
    }
}

impl State {
    #[cfg(test)]
    pub(crate) fn set_fixture_files(&mut self, files: Vec<(String, Vec<u8>)>) {
        self.fixture_files = Some(files);
    }

    /// Load the client role once, on the client setup thread, and retain every host for later G1 dispatch.
    pub(crate) fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.retired.clear();
        self.world_loaded = false;
        self.loaded = true;
        let mut parsed = Vec::new();
        let files = {
            #[cfg(test)]
            if let Some(files) = &self.fixture_files {
                files.clone()
            } else {
                crate::content::read_jar_files()
            }
            #[cfg(not(test))]
            {
                crate::content::read_jar_files()
            }
        };
        for (file, bytes) in files {
            match NativeJar::from_bytes(&bytes) {
                Ok(jar) => parsed.push((file, Arc::new(jar))),
                Err(error) => self.refused.push(Row::refused(
                    &file,
                    file.strip_suffix(".jar").unwrap_or(&file).to_owned(),
                    format!("not a readable jar: {error}"),
                )),
            }
        }
        let refs: Vec<_> = parsed
            .iter()
            .map(|(file, jar)| (file.as_str(), jar.as_ref()))
            .collect();
        let plan = runtime::dependencies::plan(&refs);
        let imc = runtime::neoforge_imc::ImcBus::new();
        for &index in &plan.order {
            let (file, jar) = parsed[index].clone();
            if let Some(missing) = plan.missing.get(&index) {
                self.refused.push(Row::refused(
                    &file,
                    jar_id(&jar, &file),
                    format!(
                        "requires {} (not in config/horizon_java/java-mods)",
                        missing.join(", ")
                    ),
                ));
                continue;
            }
            let links: Vec<_> = plan
                .links_for(index)
                .iter()
                .map(|&dependency| {
                    let (name, jar) = &parsed[dependency];
                    (name.clone(), Arc::clone(jar))
                })
                .collect();
            match load_jar(file, jar, &links, &imc) {
                Ok(jar) => self.jars.push(jar),
                Err(row) => self.refused.push(row),
            }
        }
        for jar in &mut self.jars {
            if let Some(host) = imc.reclaim(&jar.row.file) {
                jar.host = host;
            }
        }
        for jar in &self.jars {
            log_registration(&jar.row);
        }
        for row in &self.refused {
            log::info!(
                "{LOG}: client {} ({}) refused: {}",
                row.file,
                row.id,
                row.reason.as_deref().unwrap_or("unknown reason")
            );
        }
        log::info!(
            "{LOG}: client registration inventory: {} jar(s), {} loaded, {} refused",
            self.jars.len() + self.refused.len(),
            self.jars.len(),
            self.refused.len()
        );
    }

    #[cfg(test)]
    pub(crate) fn resident_count(&self) -> usize {
        self.jars.len()
    }

    #[cfg(test)]
    pub(crate) fn world_loaded(&self) -> bool {
        self.world_loaded
    }

    pub(crate) fn report_rows(&self) -> impl Iterator<Item = serde_json::Value> + '_ {
        self.retired
            .iter()
            .map(Row::json)
            .chain(self.jars.iter().map(|jar| jar.row.json()))
            .chain(self.refused.iter().map(Row::json))
    }

    pub(crate) fn level_loaded(&mut self) {
        self.world_loaded = true;
    }

    pub(crate) fn level_unloaded(&mut self) {
        self.world_loaded = false;
    }

    /// Drop resident hosts, listeners, and jar heaps at client shutdown. Rows stay available
    /// until the next client setup so the final report can be rewritten after teardown.
    pub(crate) fn teardown(&mut self) -> usize {
        let mut rows = std::mem::take(&mut self.refused);
        rows.extend(self.jars.drain(..).map(|jar| jar.row));
        let count = rows.len();
        self.retired.extend(rows);
        self.loaded = false;
        self.world_loaded = false;
        count
    }
}

fn log_registration(row: &Row) {
    log::info!(
        "{LOG}: client registration {} ({}): keys={}, hud_layers={}, listeners={}, unfired={}, refused={}",
        row.file,
        row.id,
        row.keys,
        row.hud_layers,
        row.listeners,
        row.unsupported.values().sum::<usize>(),
        row.refused_registrations.len()
    );
}

#[allow(clippy::result_large_err)]
fn load_jar(
    file: String,
    jar: Arc<NativeJar>,
    links: &[(String, Arc<NativeJar>)],
    imc: &runtime::neoforge_imc::ImcBus,
) -> Result<LoadedJar, Row> {
    let id = jar_id(&jar, &file);
    let entrypoints = jar.role_entrypoints(true);
    if entrypoints.is_empty() {
        return Err(Row::refused(&file, id, "no client entrypoint"));
    }
    let mut host = match jar.try_seed_host(StdHost::new().with_environment_type("CLIENT")) {
        Ok(host) => host,
        Err(error) => return Err(Row::refused(&file, id, error)),
    };
    imc.join(&file, &jar, &mut host);
    for (link, dependency) in links {
        if let Err(error) = host.link_jar(link.clone(), dependency) {
            return Err(Row::refused(
                &file,
                id,
                format!("linking {link} failed: {error}"),
            ));
        }
    }
    let mut row = Row {
        file,
        id: id.clone(),
        verdict: "ran-clean",
        ..Row::default()
    };
    let mut failures = Vec::new();
    let mut failed_classes = Vec::new();
    for (kind, entry) in &entrypoints {
        let counts = row.stages.entry((*kind).to_owned()).or_default();
        counts.attempted += 1;
        if let Some(adapter) = entry.adapter.as_deref() {
            row.entrypoints_failed += 1;
            counts.failed += 1;
            counts.disabled += 1;
            failures.push(format!(
                "{}: entrypoint adapter {adapter} is not supported",
                entry.class
            ));
            continue;
        }
        match caught(
            &format!("{} client entrypoint {} ({kind})", row.id, entry.class),
            || {
                jar.execute_with_budget(
                    &entry.class,
                    entry.method.as_deref(),
                    kind,
                    &mut host,
                    &ExecutionBudget::within(JAR_BUDGET),
                )
            },
        ) {
            Ok(Ok(report)) => {
                row.entrypoints_ran += 1;
                row.host_calls += report.calls;
                if let Some(counts) = row.stages.get_mut(*kind) {
                    counts.calls += report.calls;
                }
            }
            Ok(Err(error)) => {
                row.entrypoints_failed += 1;
                if let Some(counts) = row.stages.get_mut(*kind) {
                    counts.failed += 1;
                    counts.disabled += 1;
                }
                failed_classes.push(entry.class.as_str());
                failures.push(format!("{}: {error}", entry.class));
            }
            Err(panic) => {
                row.entrypoints_failed += 1;
                if let Some(counts) = row.stages.get_mut(*kind) {
                    counts.failed += 1;
                    counts.disabled += 1;
                }
                failed_classes.push(entry.class.as_str());
                failures.push(format!("{}: panicked: {panic}", entry.class));
            }
        }
    }
    if row.entrypoints_ran == 0 {
        row.verdict = "refused";
        row.reason = Some(failures.join("; "));
        row.unknown_host_calls = host.unknown_host_call_count();
        return Err(row);
    }
    for problem in jar.inject_event_subscribers(
        &mut host,
        &ExecutionBudget::within(JAR_BUDGET),
        &failed_classes,
    ) {
        failures.push(problem);
    }
    imc.lend(&row.file, Arc::clone(&jar), host);
    let mut loaded = LoadedJar {
        row,
        jar,
        host: StdHost::new().with_environment_type("CLIENT"),
        listeners: Vec::new(),
        orderings: Vec::new(),
    };
    for stage in runtime::neoforge_lifecycle::LoadingStage::ALL {
        if let Some(host) = imc.reclaim(&loaded.row.file) {
            loaded.host = host;
        }
        let stage_name = stage.name().to_owned();
        let counts = loaded.row.stages.entry(stage_name.clone()).or_default();
        counts.attempted += 1;
        let before_calls = loaded.host.calls().len();
        let budget = ExecutionBudget::within(JAR_BUDGET);
        let (jar, host) = (&loaded.jar, &mut loaded.host);
        let stage_failures = caught(&format!("{} {}", loaded.row.id, stage.name()), || {
            jar.run_loading_stage(stage, host, &budget)
        })
        .unwrap_or_else(|panic| vec![format!("{}: panicked: {panic}", stage.name())]);
        let deferred_failures = caught(
            &format!("{} {} deferred work", loaded.row.id, stage.name()),
            || jar.run_deferred_work(host, &budget),
        )
        .unwrap_or_else(|panic| vec![format!("{} deferred work panicked: {panic}", stage.name())]);
        let stage_failed = !stage_failures.is_empty() || !deferred_failures.is_empty();
        let calls = loaded.host.calls().len().saturating_sub(before_calls);
        let counts = loaded.row.stages.get_mut(&stage_name).expect("stage count");
        counts.calls += calls;
        if stage_failed {
            counts.failed += 1;
            counts.disabled += 1;
        }
        loaded.row.host_calls += calls;
        failures.extend(stage_failures);
        failures.extend(deferred_failures);
        imc.lend(
            &loaded.row.file,
            Arc::clone(&loaded.jar),
            std::mem::take(&mut loaded.host),
        );
    }
    loaded.host = imc
        .reclaim(&loaded.row.file)
        .unwrap_or_else(|| StdHost::new().with_environment_type("CLIENT"));
    let registered = loaded.host.game_listeners(&loaded.jar, false);
    loaded.listeners = registered.listeners;
    loaded.orderings = registered.phase_orderings;
    let (layers, refused) = loaded.host.client_hud_layers(&loaded.jar);
    loaded.row.keys = loaded.host.client_key_mappings().len();
    loaded.row.hud_layers = layers.len();
    loaded.row.listeners = loaded.listeners.len();
    loaded.row.unsupported = registered.unsupported;
    loaded.row.refused_registrations = loaded
        .host
        .client_refused_registrations()
        .iter()
        .cloned()
        .chain(refused)
        .collect();
    loaded.row.unknown_host_calls = loaded.host.unknown_host_call_count();
    if !failures.is_empty() {
        loaded.row.verdict = "ran-partial";
        loaded.row.reason = Some(failures.join("; "));
    } else if loaded.row.unknown_host_calls > 0 {
        loaded.row.verdict = "ran-hollow";
    }
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn jar(resources: &[(&str, &[u8])]) -> NativeJar {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut bytes);
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in resources {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        NativeJar::from_bytes(&bytes.into_inner()).unwrap()
    }

    #[test]
    fn fabric_client_entrypoint_fixture_is_selected_for_client_owner() {
        let jar = jar(&[(
            "fabric.mod.json",
            br#"{"schemaVersion":1,"id":"fixture","version":"1","entrypoints":{"client":["fixture.Client"]}}"#,
        )]);
        let entries = jar.role_entrypoints(true);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "client");
        assert_eq!(entries[0].1.class, "fixture.Client");
        assert!(jar.role_entrypoints(false).is_empty());
    }

    #[test]
    fn neoforge_client_mod_fixture_is_selected_for_client_owner() {
        let class = client_mod_class();
        let jar = jar(&[
            (
                "META-INF/neoforge.mods.toml",
                br#"modLoader = "javafml"
loaderVersion = "[1,)"
license = "MIT"
[[mods]]
modId = "fixture"
version = "1"
"#,
            ),
            ("fixture/ClientMod.class", &class),
        ]);
        let entries = jar.role_entrypoints(true);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "neoforge");
        assert_eq!(entries[0].1.class, "fixture.ClientMod");
        assert!(jar.role_entrypoints(false).is_empty());
    }

    fn client_mod_class() -> Vec<u8> {
        fn u2(out: &mut Vec<u8>, value: u16) {
            out.extend_from_slice(&value.to_be_bytes());
        }
        fn utf8(out: &mut Vec<u8>, value: &str) {
            out.push(1);
            u2(out, value.len() as u16);
            out.extend_from_slice(value.as_bytes());
        }
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52];
        u2(&mut out, 12);
        utf8(&mut out, "fixture/ClientMod");
        out.extend_from_slice(&[7, 0, 1]);
        utf8(&mut out, "java/lang/Object");
        out.extend_from_slice(&[7, 0, 3]);
        utf8(&mut out, "Lnet/neoforged/fml/common/Mod;");
        utf8(&mut out, "RuntimeVisibleAnnotations");
        utf8(&mut out, "value");
        utf8(&mut out, "fixture");
        utf8(&mut out, "dist");
        utf8(&mut out, "Lnet/neoforged/api/distmarker/Dist;");
        utf8(&mut out, "CLIENT");
        out.extend_from_slice(&[0, 0x21, 0, 2, 0, 4, 0, 0, 0, 0, 0, 0]);
        u2(&mut out, 1);
        u2(&mut out, 6);
        let mut annotation = Vec::new();
        u2(&mut annotation, 1);
        u2(&mut annotation, 5);
        u2(&mut annotation, 2);
        u2(&mut annotation, 7);
        annotation.extend_from_slice(&[b's', 0, 8]);
        u2(&mut annotation, 9);
        annotation.push(b'[');
        u2(&mut annotation, 1);
        annotation.push(b'e');
        u2(&mut annotation, 10);
        u2(&mut annotation, 11);
        out.extend_from_slice(&(annotation.len() as u32).to_be_bytes());
        out.extend_from_slice(&annotation);
        out
    }
}
