# Item combiner menu oracle

These files are independent Java 25 observations of the 26.3 server jar. The
jar is read from the local 26.3 install while generating the output and is not
part of this repository. `TwoInputMenuOracle` constructs a mod-shaped subclass
whose definition is built by its own bytecode. `SmithingMenuOracle` observes
vanilla Smithing's definition factory. The captured text has the launcher log
prefix removed so fixtures can compare it directly.

The source tree carries a vendored copy of the Smithing capture because the
repository vendor check requires literal `include_str!` inputs under `src/`.
