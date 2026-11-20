//! A source-built mod subclass builds its own two-input definition and runs the
//! vanilla ItemCombinerMenu construction through the interpreter. The expected
//! rows are captured from a real Java 25 run of the 26.3 server jar.

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

macro_rules! sources {
    ($($path:literal => $body:expr),* $(,)?) => {{
        let mut map = BTreeMap::new();
        $(map.insert($path, $body);)*
        map
    }};
}

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn write_sources(root: &Path, sources: &BTreeMap<&str, &str>) -> Vec<PathBuf> {
    sources
        .iter()
        .map(|(relative, source)| {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, source).unwrap();
            path
        })
        .collect()
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

fn compile(out: &Path, classpath: Option<&Path>, sources: &[PathBuf]) {
    fs::create_dir_all(out).unwrap();
    let mut command = Command::new(javac());
    command.args(["--release", "17", "-d"]).arg(out);
    if let Some(classpath) = classpath {
        command.args(["-cp"]).arg(classpath);
    }
    assert!(command.args(sources).status().unwrap().success());
}

fn build_jar() -> (scratch::Scratch, PathBuf) {
    let root = scratch::Scratch::new("item-combiner-menu");
    let stubs = sources! {
        "net/minecraft/core/NonNullList.java" => r#"package net.minecraft.core;
import java.util.ArrayList;
public class NonNullList<T> extends ArrayList<T> { }
"#,
        "net/minecraft/world/item/ItemStack.java" => "package net.minecraft.world.item; public class ItemStack { public static final ItemStack EMPTY = new ItemStack(); }",
        "net/minecraft/world/level/block/state/BlockState.java" => "package net.minecraft.world.level.block.state; public class BlockState { }",
        "net/minecraft/world/entity/player/Player.java" => "package net.minecraft.world.entity.player; public class Player { }",
        "net/minecraft/world/entity/player/Inventory.java" => r#"package net.minecraft.world.entity.player;
import net.minecraft.world.entity.EntityEquipment;
public class Inventory { public final Player player; public Inventory(Player p, EntityEquipment e) { player = p; } }
"#,
        "net/minecraft/world/entity/EntityEquipment.java" => "package net.minecraft.world.entity; public class EntityEquipment { }",
        "net/minecraft/world/Container.java" => r#"package net.minecraft.world; import net.minecraft.world.item.ItemStack;
public interface Container { int getContainerSize(); ItemStack getItem(int index); void setItem(int index, ItemStack stack); void setChanged(); boolean isEmpty(); }
"#,
        "net/minecraft/world/SimpleContainer.java" => "package net.minecraft.world; public class SimpleContainer implements Container { public SimpleContainer(int size) {} public int getContainerSize() { return 0; } public net.minecraft.world.item.ItemStack getItem(int i) { return null; } public void setItem(int i, net.minecraft.world.item.ItemStack s) { setChanged(); } public void setChanged() {} public boolean isEmpty() { return true; } }",
        "net/minecraft/world/inventory/ResultContainer.java" => "package net.minecraft.world.inventory; public class ResultContainer implements net.minecraft.world.Container { public ResultContainer() {} public int getContainerSize() { return 1; } public net.minecraft.world.item.ItemStack getItem(int i) { return null; } public void setItem(int i, net.minecraft.world.item.ItemStack s) {} public void setChanged() {} public boolean isEmpty() { return true; } }",
        "net/minecraft/world/inventory/MenuType.java" => "package net.minecraft.world.inventory; public class MenuType<T> { }",
        "net/minecraft/world/inventory/ContainerLevelAccess.java" => r#"package net.minecraft.world.inventory;
import java.util.Optional; import java.util.function.BiFunction;
public class ContainerLevelAccess {
  public static final ContainerLevelAccess NULL = new ContainerLevelAccess();
  public <T> Optional<T> evaluate(BiFunction<Object, Object, T> f) { return Optional.empty(); }
  public <T> T evaluate(BiFunction<Object, Object, T> f, T fallback) { return evaluate(f).orElse(fallback); }
}
"#,
        "net/minecraft/world/inventory/Slot.java" => r#"package net.minecraft.world.inventory;
import net.minecraft.world.Container;
public class Slot { public final Container container; public int index; public final int x, y; public Slot(Container c, int i, int x, int y) { container=c; index=i; this.x=x; this.y=y; } public int getContainerSlot() { return index; } public boolean mayPlace(net.minecraft.world.item.ItemStack stack) { return true; } }
"#,
        "net/minecraft/world/inventory/AbstractContainerMenu.java" => r#"package net.minecraft.world.inventory;
import net.minecraft.core.NonNullList;
public abstract class AbstractContainerMenu {
  public final NonNullList<Slot> slots = new NonNullList<>();
  protected AbstractContainerMenu(MenuType<?> type, int id) { }
  public Slot getSlot(int index) { return slots.get(index); }
  public int getResultSlot() { return 0; }
}
"#,
        "net/minecraft/world/inventory/ItemCombinerMenuSlotDefinition.java" => r#"package net.minecraft.world.inventory;
import java.util.function.Predicate; import net.minecraft.world.item.ItemStack;
public class ItemCombinerMenuSlotDefinition {
  public static Builder create() { return new Builder(); }
  public int getNumOfInputSlots() { return 0; }
  public int getResultSlotIndex() { return 0; }
  public SlotDefinition getResultSlot() { return null; }
  public static class Builder {
    public Builder() { }
    public Builder withSlot(int index, int x, int y, Predicate<ItemStack> p) { return this; }
    public Builder withResultSlot(int index, int x, int y) { return this; }
    public ItemCombinerMenuSlotDefinition build() { return null; }
  }
  public static class SlotDefinition { public int slotIndex() { return 0; } public int x() { return 0; } public int y() { return 0; } }
}
"#,
        "net/minecraft/world/inventory/ItemCombinerMenu.java" => r#"package net.minecraft.world.inventory;
import net.minecraft.world.Container; import net.minecraft.world.entity.player.Inventory; import net.minecraft.world.item.ItemStack; import net.minecraft.world.level.block.state.BlockState; import net.minecraft.world.entity.player.Player;
public abstract class ItemCombinerMenu extends AbstractContainerMenu {
  protected final ContainerLevelAccess access; protected final Player player; protected final Container inputSlots; protected final ResultContainer resultSlots;
  protected ItemCombinerMenu(MenuType<?> type, int id, Inventory inventory, ContainerLevelAccess access, ItemCombinerMenuSlotDefinition definition) { super(type, id); this.access=access; this.player=inventory == null ? null : inventory.player; this.inputSlots=null; this.resultSlots=null; }
  public boolean stillValid(Player p) { return access.evaluate((level, pos) -> Boolean.FALSE, Boolean.TRUE); }
  public abstract void createResult(); protected abstract void onTake(Player p, ItemStack s); protected abstract boolean isValidBlock(BlockState s);
}
"#,
        "net/minecraft/world/item/crafting/RecipeManager.java" => r#"package net.minecraft.world.item.crafting;
public class RecipeManager { public RecipeManager() { } public Object getRecipeFor() { return null; } }
"#,
        "demo/TwoInputMenu.java" => r#"package demo;
import net.minecraft.world.inventory.*; import net.minecraft.world.item.ItemStack; import net.minecraft.world.level.block.state.BlockState; import net.minecraft.world.entity.player.Player;
public final class TwoInputMenu extends ItemCombinerMenu {
  public TwoInputMenu() { super(null, 17, null, ContainerLevelAccess.NULL, createDefinition()); }
  private static ItemCombinerMenuSlotDefinition createDefinition() {
    return ItemCombinerMenuSlotDefinition.create().withSlot(0, 17, 29, stack -> stack != ItemStack.EMPTY).withSlot(1, 73, 29, stack -> true).withResultSlot(2, 121, 29).build();
  }
  boolean created;
  @Override public void createResult() { created = true; resultSlots.setItem(0, ItemStack.EMPTY); }
  @Override protected void onTake(Player p, ItemStack s) { }
  @Override protected boolean isValidBlock(BlockState s) { return true; }
  public void trigger() { inputSlots.setItem(0, ItemStack.EMPTY); }
  public boolean resultSet() { return created; }
  public boolean inputsEmpty() { return inputSlots.isEmpty(); }
  public boolean resultEmpty() { return resultSlots.isEmpty(); }
}
"#,
        "demo/Main.java" => r#"package demo;
public final class Main {
  private static void validation(String name, Runnable action) {
    try { action.run(); System.out.println(name + "=ok"); }
    catch (Throwable error) { System.out.println(name + "=" + error.getClass().getName() + ":" + error.getMessage()); }
  }
  public static void onInitialize() {
    TwoInputMenu menu = new TwoInputMenu();
    new net.minecraft.world.item.crafting.RecipeManager().getRecipeFor();
    System.out.println("slotCount=" + menu.slots.size());
    for (int i = 0; i < menu.slots.size(); i++) {
      net.minecraft.world.inventory.Slot slot = menu.getSlot(i);
      System.out.println("slot position=" + i + " index=" + slot.index + " x=" + slot.x + " y=" + slot.y + " containerSlot=" + slot.getContainerSlot());
    }
    int result = menu.getResultSlot();
    System.out.println("resultSlot=" + result);
    System.out.println("inventoryStart=" + (result + 1));
    System.out.println("inventoryEnd=" + (result + 1 + 27));
    System.out.println("useRowStart=" + (result + 1 + 27));
    System.out.println("useRowEnd=" + (result + 1 + 27 + 9));
    System.out.println("stillValid=" + menu.stillValid(null));
    System.out.println("inputEmpty=" + menu.inputsEmpty());
    System.out.println("resultEmpty=" + menu.resultEmpty());
    menu.trigger();
    System.out.println("resultSet=" + menu.resultSet());
    System.out.println("mayPlace=" + menu.getSlot(0).mayPlace(net.minecraft.world.item.ItemStack.EMPTY));
    validation("builderEmpty", () -> net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition.create().build());
    validation("builderUnsetResult", () -> net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition.create().withSlot(0, 0, 0, stack -> true).build());
    validation("builderBadInputIndex", () -> net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition.create().withSlot(1, 0, 0, stack -> true).withResultSlot(2, 0, 0).build());
    validation("builderBadResultIndex", () -> net.minecraft.world.inventory.ItemCombinerMenuSlotDefinition.create().withSlot(0, 0, 0, stack -> true).withResultSlot(2, 0, 0).build());
  }
}
"#,
    };
    let stub_src = write_sources(&root.root().join("stub-src"), &stubs);
    let stub_classes = root.root().join("stub-classes");
    compile(&stub_classes, None, &stub_src);
    let mut mod_sources = stubs.clone();
    mod_sources.retain(|name, _| name.starts_with("demo/"));
    let mod_src = write_sources(&root.root().join("mod-src"), &mod_sources);
    let classes = root.root().join("classes");
    compile(&classes, Some(&stub_classes), &mod_src);
    let jar_path = root.root().join("item-combiner-menu.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    zip.start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(
        br#"{"id":"fixture-item-combiner","version":"1","entrypoints":{"main":"demo.Main"}}"#,
    )
    .unwrap();
    for class in files_under(&classes) {
        let entry = class
            .strip_prefix(&classes)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(entry, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(class).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    (root, jar_path)
}

#[test]
fn mod_owned_item_combiner_definition_runs_through_interpreter() {
    let (_root, jar_path) = build_jar();
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    if let Err(error) = jar.execute("demo.Main", None, "main", &mut host) {
        eprintln!("error={error:?} unknown={:?}", host.unknown_host_calls());
        panic!("{error:?}");
    }
    assert_eq!(
        host.take_stdout(),
        include_str!("fixtures/item_combiner_menu/java25.txt")
    );
    assert!(host.unknown_host_calls().iter().any(|(call, _)| {
        call.starts_with("net/minecraft/world/item/crafting/RecipeManager#getRecipeFor")
    }));
}
