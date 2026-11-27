#!/usr/bin/env python3
"""Record where the 26.3 bootstrap runs each Mixin `TAIL` callback into a game class.

Writes, for the server (`--side server`, the default) or the client (`--side client`):

* `crates/horizon-native-java/src/bootstrap_clinit_order_26_3.txt`: the server's, while
  `net.minecraft.server.Main.main`'s prelude runs (`SharedConstants.tryDetectVersion()`,
  `CrashReport.preload()`, `Bootstrap.bootStrap()`, the calls it makes before anything reads its
  options);
* `crates/horizon-native-java/src/bootstrap_clinit_order_26_3_client.txt`: the client's, while the
  real `net.minecraft.client.main.Main.main` runs, up to the first instruction of
  `Minecraft.<init>` (where Fabric Loader runs the first `main` and `client` entrypoints, so
  everything before it happens before any mod's code; the client's own `Minecraft.<clinit>`
  included).

Each line is one point a `TAIL` callback runs at, in the order the game reaches them: a class's
internal name when its `<clinit>` returns, and `<class>.<name>()V` when a static no-argument
`void` method reaches its last `return` (Mixin's `TAIL` anchor; only the shape a
`(CallbackInfo)V` handler can mirror). A method that ends more than once is listed each time.

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
real client `Main` runs with its own launcher arguments (an offline user, a scratch game folder)
and the agent writes the queue and halts the JVM at `Minecraft.<init>`'s first instruction, before
the game opens a window or touches the network (a SOCKS proxy at a closed local port backs that
up). A point reached on a thread other than `main` is marked `@other-thread`: its time is not the
main thread's to order, and the runtime counts a callback there. An initializer that throws never
returns and is not listed. Neither run has inputs that vary (no world, no options file, no
account), so two runs give the same list. Fabric does not change it: it loads game classes
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

    python3 tools/trace_bootstrap_clinit_order.py --side client \\
        --client-jar <the launcher's versions/26.3/26.3.jar> \\
        --version-json <the launcher's versions/26.3/26.3.json> \\
        --libraries <the launcher's libraries directory> \\
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
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

GAME_VERSION = "26.3"
JDK_FEATURE = "25"

SERVER_HEADER = """\
# Mixin TAIL points the 26.3 server's bootstrap reaches, in the order it reaches them.
# Minecraft 26.3 server, JDK 25: SharedConstants.tryDetectVersion(), CrashReport.preload(),
# Bootstrap.bootStrap() (net.minecraft.server.Main.main's prelude). Generated by
# tools/trace_bootstrap_clinit_order.py (a java.lang.classfile agent); do not edit by hand. The
# `# input` lines are the exact inputs, which the tool checks before every run. Then one point per
# line: a class's internal name where its <clinit> returns, `<class>.<name>()V` where a static
# ()V method reaches its last return.
"""

CLIENT_HEADER = """\
# Mixin TAIL points the 26.3 client reaches before Minecraft.<init> (where Fabric runs the first
# entrypoint), in the order it reaches them. Minecraft 26.3 client, JDK 25: the real
# net.minecraft.client.main.Main.main (offline user, scratch game folder), stopped at the first
# instruction of Minecraft.<init>. Generated by tools/trace_bootstrap_clinit_order.py --side client
# (a java.lang.classfile agent); do not edit by hand. The `# input` lines are the exact inputs,
# which the tool checks before every run. Then one point per line: a class's internal name where
# its <clinit> returns, `<class>.<name>()V` where a static ()V method reaches its last return.
"""

AGENT_SOURCE = """\
import java.lang.classfile.*;
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
 * static `()V` method (`InitEnd.done("<class>.<name>()V")`). With an argument, stops the JVM at
 * the first instruction of the class:method it names (`InitEnd.finish()`).
 */
public class InitEndAgent {
    public static void premain(String args, Instrumentation inst) {
        String[] parts = args.split("\\\\|", -1);
        InitEnd.output = parts[0];
        String stopClass = parts.length > 1 ? parts[1] : "";
        String stopMethod = parts.length > 2 ? parts[2] : "";
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
                        String label = clinit ? name : name + "." + methodName + "()V";
                        long returns = method.code().get().elementStream()
                            .filter(e -> e instanceof ReturnInstruction).count();
                        builder.transformMethod(method, MethodTransform.transformingCode(
                            new CodeTransform() {
                                long seen = 0;
                                @Override
                                public void atStart(CodeBuilder code) {
                                    if (stop) {
                                        code.invokestatic(ClassDesc.of("InitEnd"), "finish",
                                            MethodTypeDesc.ofDescriptor("()V"));
                                    }
                                }
                                @Override
                                public void accept(CodeBuilder code, CodeElement e) {
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
                    return cf.transformClass(model, transform);
                } catch (Throwable t) {
                    System.err.println("INITEND-TRANSFORM-FAILED " + name + " " + t);
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


def run_traced(jdk: Path, agent: Path, stop: str, class_path: str, main: list[str],
               work: Path, listing: Path) -> list[str]:
    """Run `main` under the agent (stopping at `stop`, `class|method`, if given) and return the
    points it recorded."""
    tmp = work / "tmp"
    tmp.mkdir(exist_ok=True)
    run = subprocess.run(
        [str(jdk / "bin" / "java"),
         # Nothing before the stop point needs the network; a closed local SOCKS port makes
         # sure nothing reaches it.
         "-DsocksProxyHost=127.0.0.1", "-DsocksProxyPort=9",
         f"-Djava.io.tmpdir={tmp}", f"-Dorg.lwjgl.system.SharedLibraryExtractPath={tmp / 'lwjgl'}",
         f"-javaagent:{agent}={listing}|{stop}", "-cp", f"{class_path}:{agent}", *main],
        cwd=work,
        capture_output=True,
        text=True,
        timeout=900,
    )
    if run.returncode != 0:
        raise SystemExit(f"the bootstrap run failed ({run.returncode}):\n{run.stderr[-4000:]}")
    output = run.stdout + run.stderr
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
    """Run the real client `Main` under the agent up to `Minecraft.<init>` and return the points
    it reached."""
    agent = build_agent(jdk, work)
    game = work / "game"
    game.mkdir()
    class_path = ":".join([*(str(lib) for lib in libs), str(client_jar)])
    main = ["net.minecraft.client.main.Main", "--version", GAME_VERSION, "--accessToken", "0",
            "--username", "Tracer", "--gameDir", str(game), "--assetsDir", str(work / "assets")]
    return run_traced(jdk, agent, "net/minecraft/client/Minecraft|<init>", class_path, main,
                      work, work / "done.txt")


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
