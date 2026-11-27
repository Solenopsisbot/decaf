#!/usr/bin/env python3
"""Record where the 26.3 bootstrap runs each Mixin `TAIL` callback into a game class.

Writes, for the server (`--side server`, the default) or the client (`--side client`):

* `crates/horizon-native-java/src/bootstrap_clinit_order_26_3.txt`: the server's, while
  `net.minecraft.server.Main.main`'s prelude runs (`SharedConstants.tryDetectVersion()`,
  `CrashReport.preload()`, `Bootstrap.bootStrap()`, the calls it makes before anything reads its
  options);
* `crates/horizon-native-java/src/bootstrap_clinit_order_26_3_client.txt`: the client's, while the
  real `net.minecraft.client.main.Main.main` runs, up to the point in `Minecraft.<init>` where
  Fabric Loader 0.20.1 runs the first `main` and `client` entrypoints (`EntrypointPatch`: just
  before the last `Thread.currentThread()` call ahead of the "Backend library: {}" log line,
  offset 555), so everything listed happens before any mod's code: the client's own
  `Minecraft.<clinit>`, and the first part of its constructor (pack repository, services, user
  API service, data fixers; `Options`, `Tutorial` and `HotbarManager` come after the hook).

Each line is one point a `TAIL` callback runs at, in the order the game reaches them: a class's
internal name when its `<clinit>` returns, and `<class>.<name>()V` when a static no-argument
`void` method reaches its last `return` (Mixin's `TAIL` anchor). A method that ends more than
once is listed each time; one whose name its class declares more than once (an overload a
name-only selector would also select) is marked `@overloaded`.

Why the runtime needs it
------------------------
A Mixin `@Inject(method = "<clinit>", at = @At("TAIL"))` into a game class runs once, as the last
thing that class's static initializer does. Bundle API registers its data component type that way
(`DataComponentTypesMixin` into `DataComponents`, `Registry.register(BuiltInRegistries
.DATA_COMPONENT_TYPE, ...)`), and its client numeric item model property with a `TAIL` into
`RangeSelectItemModelProperties.bootstrap()`. The runtime models game classes on the host, so their
initializers and bootstrap methods never run there; it runs such callbacks itself before a jar's
first entrypoint (`bootstrap_mixins.rs`). When callbacks target several points, they run in the
order the real game reaches them, which is this list: a `TAIL` injector runs at its class's
*completion*, and one class's initializer can start another's, so the JVM's `-Xlog:class+init`
(which logs starts only) can't give it. The client's order is not the server's (its `Main` runs
other code first, and `ClientBootstrap.bootstrap()` after `Bootstrap.bootStrap()`), so each side
has its own list.

How it is measured
------------------
A Java agent (`InitEndAgent`, JDK 25's `java.lang.classfile`) rewrites every `net/minecraft/`
class: each `<clinit>` `return` first calls `InitEnd.done("<class>")`, and the last `return` of
each static `()V` method `InitEnd.done("<class>.<name>()V")`, which append to a queue. On the
server, a `main` mirroring `Main.main`'s prelude runs, then writes the queue. On the client, the
real client `Main` runs with its own launcher arguments (an offline user, a scratch game folder,
a proxy at a closed local port) and the agent writes the queue and halts the JVM just before
Fabric's entrypoint hook, before the game opens a window. Both sides run in a network namespace
with no network (`unshare -rn`; the tool refuses to run without one), with no display and with
`HOME` and the XDG folders inside the temporary work folder. The window up to the hook builds the
game's services: `createUserApiService` (`offlineDeveloperMode` is off) makes no request itself
(authlib 10.0.77 builds a `MinecraftServicesUserApiService` over the proxy), and the two
`CompletableFuture.supplyAsync` calls on `Util.nonCriticalIoPool()` run on IO threads, whose
points are marked. A point reached on a thread other than `main` is marked `@other-thread`: its
time is not the main thread's to order, and the runtime counts a callback there. An initializer
that throws never returns and is not listed. Neither run has inputs that vary (no world, no
options file, no account, no network), so two runs give the same list. Fabric does not change it: it loads game classes
through Knot and Mixin's transformer, but initializes them in the game's order, and runs `main`
entrypoints only after this bootstrap (`fabric_server_bootstrap_classes_26_3.txt` lists
`DataComponents` among the classes a real Fabric server loads before the first entrypoint).

Reproducing the committed files byte-for-byte
---------------------------------------------
    python3 tools/trace_bootstrap_clinit_order.py \\
        --server-jar <unbundled Minecraft 26.3 server jar> \\
        --libs <the bundle's META-INF/libraries directory> \\
        --jdk <JDK 25 home, e.g. Temurin 25.0.5+8> \\
        --output crates/horizon-native-java/src/bootstrap_clinit_order_26_3.txt

    # Copy the launcher's files into a scratch folder first; the tool never reads them in place.
    cp -r ~/.minecraft/versions/26.3 ~/.minecraft/libraries <scratch>/
    python3 tools/trace_bootstrap_clinit_order.py --side client \\
        --client-jar <scratch>/26.3/26.3.jar \\
        --version-json <scratch>/26.3/26.3.json \\
        --libraries <scratch>/libraries \\
        --jdk <JDK 25 home> \\
        --output crates/horizon-native-java/src/bootstrap_clinit_order_26_3_client.txt

The game jar's `version.json` must name 26.3 and the JDK must be 25; the game jar's SHA-256 and
the JDK's `java.runtime.version` are recorded as `# input` lines and checked on every later run
(`--record` writes new ones, for a deliberate change of inputs). The tool reads the game files and
never writes next to them: the client runs in a temporary folder.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

GAME_VERSION = "26.3"

# Where Fabric Loader 0.20.1 runs the client's first entrypoint: `EntrypointPatch` (Minecraft
# 1.19.4 and later) scans `Minecraft.<init>` for the "Backend library: {}" `ldc`, remembers the
# last `invokestatic java/lang/Thread.currentThread()` before it, and inserts
# `Hooks.startClient(gameDir, this)` just before that call (`GamePatch.moveBefore`). In 26.3 that is
# offset 555 (`this.gameThread = Thread.currentThread()`), after `DataFixers.getDataFixer()` and
# before `new Options`. The trace stops just there.
CLIENT_STOP = (
    "net/minecraft/client/Minecraft|<init>|Backend library: {}|java/lang/Thread.currentThread"
)
JDK_FEATURE = "25"

SERVER_HEADER = """\
# Mixin TAIL points the 26.3 server's bootstrap reaches, in the order it reaches them.
# Minecraft 26.3 server, JDK 25: SharedConstants.tryDetectVersion(), CrashReport.preload(),
# Bootstrap.bootStrap() (net.minecraft.server.Main.main's prelude). Generated by
# tools/trace_bootstrap_clinit_order.py (a java.lang.classfile agent); do not edit by hand. The
# `# input` lines are the exact inputs, which the tool checks before every run. Then one point per
# line: a class's internal name where its <clinit> returns, `<class>.<name>()V` where a static
# ()V method reaches its last return (` @overloaded`: the class declares that name more than
# once; ` @other-thread`: reached off the main thread).
"""

CLIENT_HEADER = """\
# Mixin TAIL points the 26.3 client reaches before Fabric runs the first entrypoint, in the order
# it reaches them. Minecraft 26.3 client, JDK 25: the real net.minecraft.client.main.Main.main
# (offline user, scratch game folder, no network), stopped in Minecraft.<init> where Fabric Loader
# 0.20.1 starts the entrypoints: just before the last Thread.currentThread() call ahead of the
# "Backend library: {}" ldc (offset 555). Generated by tools/trace_bootstrap_clinit_order.py --side client
# (a java.lang.classfile agent); do not edit by hand. The `# input` lines are the exact inputs,
# which the tool checks before every run. Then one point per line: a class's internal name where
# its <clinit> returns, `<class>.<name>()V` where a static ()V method reaches its last return
# (` @overloaded`: the class declares that name more than once; ` @other-thread`: reached off the
# main thread).
"""

AGENT_SOURCE = """\
import java.lang.classfile.*;
import java.lang.classfile.instruction.ConstantInstruction;
import java.lang.classfile.instruction.InvokeInstruction;
import java.lang.classfile.instruction.ReturnInstruction;
import java.lang.constant.ClassDesc;
import java.lang.constant.MethodTypeDesc;
import java.lang.instrument.ClassFileTransformer;
import java.lang.instrument.Instrumentation;
import java.lang.reflect.AccessFlag;
import java.security.ProtectionDomain;

/**
 * Records, in order, where a Mixin `@At("TAIL")` callback would run: before every `return` of
 * each net/minecraft `<clinit>` (`InitEnd.done("<class>")`), and before the last `return` of each
 * static `()V` method (`InitEnd.done("<class>.<name>()V")`). With a stop point
 * (`<class>|<method>|<string>|<owner>.<name>`), writes the list and halts the JVM
 * (`InitEnd.finish()`) just before the last `invokestatic <owner>.<name>` that comes before the
 * first `ldc` of that string in that method (Fabric Loader's `EntrypointPatch` places its hook
 * that way); before the `ldc` itself when no call is named, at the method's first instruction
 * when no string is. A stop point the agent cannot place halts the JVM at once (exit 4): the
 * game must never run past it.
 */
public class InitEndAgent {
    public static void premain(String args, Instrumentation inst) {
        String[] parts = args.split("\\\\|", -1);
        InitEnd.output = parts[0];
        String stopClass = parts.length > 1 ? parts[1] : "";
        String stopMethod = parts.length > 2 ? parts[2] : "";
        String stopAt = parts.length > 3 ? parts[3] : "";
        String stepBack = parts.length > 4 ? parts[4] : "";
        inst.addTransformer(new ClassFileTransformer() {
            @Override
            public byte[] transform(ClassLoader loader, String name, Class<?> redefined,
                                    ProtectionDomain domain, byte[] bytes) {
                if (name == null || !name.startsWith("net/minecraft/")) return null;
                try {
                    ClassFile cf = ClassFile.of(ClassFile.ClassHierarchyResolverOption.of(
                        ClassHierarchyResolver.defaultResolver().orElse(
                            ClassHierarchyResolver.ofResourceParsing(loader))));
                    ClassModel model = cf.parse(bytes);
                    MethodTypeDesc done = MethodTypeDesc.ofDescriptor("(Ljava/lang/String;)V");
                    ClassTransform transform = (builder, element) -> {
                        if (!(element instanceof MethodModel method) || method.code().isEmpty()) {
                            builder.with(element);
                            return;
                        }
                        String methodName = method.methodName().stringValue();
                        boolean clinit = methodName.equals("<clinit>");
                        boolean tail = !clinit && method.flags().has(AccessFlag.STATIC)
                            && method.methodType().equalsString("()V");
                        boolean stop = name.equals(stopClass) && methodName.equals(stopMethod);
                        if (!clinit && !tail && !stop) {
                            builder.with(element);
                            return;
                        }
                        // A name-only Mixin selector selects every method of that name: a point
                        // whose name the class declares more than once is marked.
                        boolean overloaded = !clinit && model.methods().stream()
                            .filter(m -> m.methodName().equalsString(methodName)).count() > 1;
                        String label = clinit ? name
                            : name + "." + methodName + "()V" + (overloaded ? " @overloaded" : "");
                        long returns = method.code().get().elementStream()
                            .filter(e -> e instanceof ReturnInstruction).count();
                        // The index, in the method's code elements, the stop goes before.
                        long stopIndex = -1;
                        if (stop && !stopAt.isEmpty()) {
                            java.util.List<CodeElement> elements =
                                method.code().get().elementStream().toList();
                            int anchor = -1;
                            for (int i = 0; i < elements.size() && anchor < 0; i++) {
                                if (elements.get(i) instanceof ConstantInstruction c
                                        && stopAt.equals(c.constantValue())) {
                                    anchor = i;
                                }
                            }
                            stopIndex = stepBack.isEmpty() ? anchor : -1;
                            for (int i = 0; i < anchor && !stepBack.isEmpty(); i++) {
                                if (elements.get(i) instanceof InvokeInstruction call
                                        && (call.owner().asInternalName() + "."
                                            + call.name().stringValue()).equals(stepBack)) {
                                    stopIndex = i;
                                }
                            }
                            if (stopIndex < 0) {
                                System.err.println("INITEND-STOP-NOT-FOUND " + name + "." + methodName);
                                Runtime.getRuntime().halt(4);
                            }
                        }
                        long stopAtIndex = stopIndex;
                        builder.transformMethod(method, MethodTransform.transformingCode(
                            new CodeTransform() {
                                long seen = 0;
                                long index = 0;
                                @Override
                                public void atStart(CodeBuilder code) {
                                    if (stop && stopAt.isEmpty()) {
                                        code.invokestatic(ClassDesc.of("InitEnd"), "finish",
                                            MethodTypeDesc.ofDescriptor("()V"));
                                    }
                                }
                                @Override
                                public void accept(CodeBuilder code, CodeElement e) {
                                    if (stop && index++ == stopAtIndex) {
                                        code.invokestatic(ClassDesc.of("InitEnd"), "finish",
                                            MethodTypeDesc.ofDescriptor("()V"));
                                    }
                                    if (e instanceof ReturnInstruction) {
                                        seen++;
                                        // `<clinit>`: every return finishes initialization.
                                        // A method's TAIL is its last return only.
                                        if (clinit || (tail && seen == returns)) {
                                            code.ldc(label);
                                            code.invokestatic(ClassDesc.of("InitEnd"), "done", done);
                                        }
                                    }
                                    code.with(e);
                                }
                            }));
                    };
                    if (name.equals(stopClass) && model.methods().stream()
                            .noneMatch(m -> m.methodName().equalsString(stopMethod))) {
                        System.err.println("INITEND-STOP-NOT-FOUND " + name + "." + stopMethod);
                        Runtime.getRuntime().halt(4);
                    }
                    return cf.transformClass(model, transform);
                } catch (Throwable t) {
                    System.err.println("INITEND-TRANSFORM-FAILED " + name + " " + t);
                    // The stop class untransformed would let the game run on past the stop.
                    if (name.equals(stopClass)) {
                        Runtime.getRuntime().halt(4);
                    }
                    return null;
                }
            }
        });
    }
}
"""

INIT_END_SOURCE = """\
import java.util.concurrent.ConcurrentLinkedQueue;

public final class InitEnd {
    static volatile String output;
    static final ConcurrentLinkedQueue<String> DONE = new ConcurrentLinkedQueue<>();
    static final Thread MAIN = Thread.currentThread();
    public static void done(String name) {
        // A callback on another thread runs at a time the main thread does not order.
        DONE.add(Thread.currentThread() == MAIN ? name : name + " @other-thread");
    }
    public static void finish() {
        try {
            java.nio.file.Files.write(java.nio.file.Path.of(output), DONE);
        } catch (Throwable t) {
            t.printStackTrace();
            Runtime.getRuntime().halt(3);
        }
        Runtime.getRuntime().halt(0);
    }
}
"""

BOOT_SOURCE = """\
public class Boot {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.CrashReport.preload();
        net.minecraft.server.Bootstrap.bootStrap();
        // The game replaces System.out with its logger: the agent writes the list to its file.
        InitEnd.finish();
    }
}
"""


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def jdk_runtime_version(jdk: Path) -> str:
    """`java.runtime.version` of the JDK at `jdk`."""
    result = subprocess.run(
        [str(jdk / "bin" / "java"), "-XshowSettings:properties", "-version"],
        capture_output=True,
        text=True,
        check=True,
    )
    for line in result.stderr.splitlines():
        key, _, value = line.strip().partition(" = ")
        if key == "java.runtime.version":
            return value
    raise SystemExit(f"no java.runtime.version from {jdk}")


def input_manifest(side: str, game_jar: Path, jdk: Path) -> list[str]:
    """The trace's inputs as `# input` header lines, after checking the versions."""
    with zipfile.ZipFile(game_jar) as archive:
        if "META-INF/versions.list" in archive.namelist():
            raise SystemExit(f"{game_jar.name} is the server bundler; pass the server jar it unpacks")
        try:
            version = json.loads(archive.read("version.json"))["id"]
        except KeyError:
            raise SystemExit(f"{game_jar.name} has no version.json: not an unbundled game jar")
        client = "net/minecraft/client/main/Main.class" in archive.namelist()
    if version != GAME_VERSION:
        raise SystemExit(f"{game_jar.name} is Minecraft {version}, not {GAME_VERSION}")
    if client != (side == "client"):
        raise SystemExit(f"{game_jar.name} is not the {side} jar")
    runtime = jdk_runtime_version(jdk)
    if runtime.split(".")[0].split("+")[0] != JDK_FEATURE:
        raise SystemExit(f"JDK {runtime} is not JDK {JDK_FEATURE}")
    return [f"# input jdk {runtime}", f"# input {side} {game_jar.name} sha256:{sha256(game_jar)}"]


def client_libraries(version_json: Path, libraries: Path) -> list[Path]:
    """The Linux class path the launcher builds from `version_json` (its `libraries` whose rules
    allow Linux), in the file's order."""
    data = json.loads(version_json.read_text(encoding="utf-8"))
    if data.get("id") != GAME_VERSION:
        raise SystemExit(f"{version_json.name} is version {data.get('id')}, not {GAME_VERSION}")
    paths = []
    for library in data["libraries"]:
        allowed = not library.get("rules")
        for rule in library.get("rules", []):
            if rule.get("os", {}).get("name", "linux") == "linux" and "features" not in rule:
                allowed = rule["action"] == "allow"
        if not allowed:
            continue
        path = libraries / library["downloads"]["artifact"]["path"]
        if not path.is_file():
            raise SystemExit(f"missing library {path}")
        paths.append(path)
    return paths


def recorded_manifest(output: Path) -> list[str] | None:
    if not output.is_file():
        return None
    return [line for line in output.read_text(encoding="utf-8").splitlines() if line.startswith("# input ")]


def build_agent(jdk: Path, work: Path) -> Path:
    """Compile `InitEndAgent` and `InitEnd` into `agent.jar` (on the class path too, so the
    rewritten game classes find `InitEnd`)."""
    sources = work / "agent-src"
    classes = work / "agent"
    sources.mkdir()
    classes.mkdir()
    (sources / "InitEndAgent.java").write_text(AGENT_SOURCE, encoding="utf-8")
    (sources / "InitEnd.java").write_text(INIT_END_SOURCE, encoding="utf-8")
    subprocess.run(
        [str(jdk / "bin" / "javac"), "-d", str(classes),
         str(sources / "InitEndAgent.java"), str(sources / "InitEnd.java")],
        check=True,
    )
    manifest = work / "manifest.txt"
    manifest.write_text("Premain-Class: InitEndAgent\n", encoding="utf-8")
    agent = work / "agent.jar"
    subprocess.run(
        [str(jdk / "bin" / "jar"), "cfm", str(agent), str(manifest), "-C", str(classes), "."],
        check=True,
    )
    return agent


def require_no_network() -> None:
    """Refuse to run unless `unshare -rn` gives this user a network namespace with no network:
    the game must never reach the network, whatever goes wrong with the stop point (26.3's
    client `Main` hands `Minecraft.<init>` `Proxy.NO_PROXY`, and authlib and `HttpClient` ignore
    the JVM's proxy settings, so a dead proxy alone is no guard)."""
    try:
        ok = subprocess.run(["unshare", "-rn", "true"], capture_output=True).returncode == 0
    except FileNotFoundError:
        ok = False
    if not ok:
        raise SystemExit("`unshare -rn true` fails here: the trace runs only without a network")


def isolated_env(work: Path) -> dict[str, str]:
    """The environment the game runs in: no display, no JVM option variables, and a home and XDG
    folders inside `work`, so nothing reads or writes the user's own (a launcher's account files,
    caches)."""
    # The JVM's own option variables would add agents or flags the trace doesn't choose.
    dropped = {"DISPLAY", "WAYLAND_DISPLAY", "JAVA_TOOL_OPTIONS", "JDK_JAVA_OPTIONS", "_JAVA_OPTIONS"}
    env = {key: value for key, value in os.environ.items()
           if key not in dropped and not key.startswith("XDG_")}
    home = work / "home"
    for name in ("", ".config", ".cache", ".local/share", ".local/state", "runtime"):
        (home / name).mkdir(parents=True, exist_ok=True)
    env.update({
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "XDG_DATA_HOME": str(home / ".local/share"),
        "XDG_STATE_HOME": str(home / ".local/state"),
        "XDG_RUNTIME_DIR": str(home / "runtime"),
    })
    return env


def run_traced(jdk: Path, agent: Path, stop: str, class_path: str, main: list[str],
               work: Path, listing: Path) -> list[str]:
    """Run `main` under the agent, in a network namespace with no network (`unshare -rn`), and
    return the points it recorded. `stop` is `class|method|string` (see the agent), or empty."""
    require_no_network()
    tmp = work / "tmp"
    tmp.mkdir(exist_ok=True)
    run = subprocess.run(
        ["unshare", "-rn", str(jdk / "bin" / "java"),
         # Belt and braces: the namespace has no network; a closed local SOCKS port too.
         "-DsocksProxyHost=127.0.0.1", "-DsocksProxyPort=9",
         f"-Djava.io.tmpdir={tmp}", f"-Dorg.lwjgl.system.SharedLibraryExtractPath={tmp / 'lwjgl'}",
         f"-Djna.tmpdir={tmp / 'jna'}", f"-Dio.netty.native.workdir={tmp / 'netty'}",
         f"-javaagent:{agent}={listing}|{stop}", "-cp", f"{class_path}:{agent}", *main],
        cwd=work,
        env=isolated_env(work),
        capture_output=True,
        text=True,
        timeout=900,
    )
    if run.returncode != 0:
        raise SystemExit(f"the bootstrap run failed ({run.returncode}):\n{run.stderr[-4000:]}")
    output = run.stdout + run.stderr
    if "INITEND-STOP-NOT-FOUND" in output:
        raise SystemExit("the agent could not place the stop point:\n" + output[-2000:])
    if "INITEND-TRANSFORM-FAILED" in output:
        failed = [line for line in output.splitlines() if "INITEND-TRANSFORM-FAILED" in line]
        raise SystemExit("the agent could not rewrite every game class:\n" + "\n".join(failed[:20]))
    if not listing.is_file():
        raise SystemExit("the run ended before the stop point")
    names = listing.read_text(encoding="utf-8").splitlines()
    clinits = [name for name in names if not name.split(" ")[0].endswith("()V")]
    if len(set(clinits)) != len(clinits):
        raise SystemExit("a class finished initializing twice")
    return names


def trace_server(jdk: Path, server_jar: Path, libs: list[Path], work: Path) -> list[str]:
    """Run the server's prelude under the agent and return the points it reached."""
    agent = build_agent(jdk, work)
    sources = work / "src"
    boot_classes = work / "boot"
    sources.mkdir()
    boot_classes.mkdir()
    (sources / "Boot.java").write_text(BOOT_SOURCE, encoding="utf-8")
    class_path = ":".join([*(str(lib) for lib in libs), str(server_jar)])
    subprocess.run(
        [str(jdk / "bin" / "javac"), "-cp", f"{class_path}:{agent}", "-d", str(boot_classes),
         str(sources / "Boot.java")],
        check=True,
    )
    return run_traced(jdk, agent, "", f"{class_path}:{boot_classes}", ["Boot"], work,
                      work / "done.txt")


def trace_client(jdk: Path, client_jar: Path, libs: list[Path], work: Path) -> list[str]:
    """Run the real client `Main` under the agent up to Fabric's entrypoint hook in
    `Minecraft.<init>` ([`CLIENT_STOP`]) and return the points it reached."""
    agent = build_agent(jdk, work)
    game = work / "game"
    game.mkdir()
    class_path = ":".join([*(str(lib) for lib in libs), str(client_jar)])
    # `--proxyHost` makes `Main` hand authlib a proxy at a closed local port instead of
    # `Proxy.NO_PROXY` (it builds the `Proxy` before the stop; nothing of it is a game class).
    main = ["net.minecraft.client.main.Main", "--version", GAME_VERSION, "--accessToken", "0",
            "--username", "Tracer", "--gameDir", str(game), "--assetsDir", str(work / "assets"),
            "--proxyHost", "127.0.0.1", "--proxyPort", "9"]
    return run_traced(jdk, agent, CLIENT_STOP, class_path, main, work, work / "done.txt")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--side", choices=["server", "client"], default="server")
    parser.add_argument("--server-jar", type=Path, help="server: the unbundled server jar")
    parser.add_argument("--libs", type=Path, help="server: the bundle's META-INF/libraries")
    parser.add_argument("--client-jar", type=Path, help="client: versions/26.3/26.3.jar")
    parser.add_argument("--version-json", type=Path, help="client: versions/26.3/26.3.json")
    parser.add_argument("--libraries", type=Path, help="client: the launcher's libraries folder")
    parser.add_argument("--jdk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--record", action="store_true", help="write a new input manifest")
    args = parser.parse_args()
    if args.side == "server":
        if args.server_jar is None or args.libs is None:
            parser.error("--side server needs --server-jar and --libs")
        game_jar = args.server_jar.resolve()
        libs = sorted(path.resolve() for path in args.libs.rglob("*.jar"))
        header = SERVER_HEADER
    else:
        if args.client_jar is None or args.version_json is None or args.libraries is None:
            parser.error("--side client needs --client-jar, --version-json and --libraries")
        game_jar = args.client_jar.resolve()
        libs = client_libraries(args.version_json, args.libraries.resolve())
        header = CLIENT_HEADER
    manifest = input_manifest(args.side, game_jar, args.jdk)
    recorded = recorded_manifest(args.output)
    if recorded is not None and recorded != manifest and not args.record:
        print("inputs differ from the recorded manifest (pass --record to accept):", file=sys.stderr)
        for line in sorted(set(recorded) ^ set(manifest)):
            print(f"  {line}", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory(prefix="clinit-order-") as folder:
        if args.side == "server":
            names = trace_server(args.jdk.resolve(), game_jar, libs, Path(folder))
        else:
            names = trace_client(args.jdk.resolve(), game_jar, libs, Path(folder))
    text = header + "".join(f"{line}\n" for line in manifest) + "".join(f"{name}\n" for name in names)
    args.output.write_text(text, encoding="utf-8")
    print(f"{len(names)} points written to {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
