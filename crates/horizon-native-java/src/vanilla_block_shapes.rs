//! Vanilla Minecraft 26.3 `Block` subclasses and whether each inherits `BlockBehaviour.getShape`
//! and `BlockBehaviour.getCollisionShape`, captured against the real 26.3 `server-26.3.jar` with
//! OpenJDK 25 (`tests/oracles/block_shapes/CaptureAllOracle.java`).
//!
//! Real 26.3 `BlockBehaviour.getShape` bytecode:
//! ```text
//!   0: invokestatic  #408 // Method net/minecraft/world/phys/shapes/Shapes.block:()LVoxelShape;
//!   3: areturn
//! ```
//!
//! Real 26.3 `BlockBehaviour.getCollisionShape` bytecode:
//! ```text
//!   0: aload_0
//!   1: getfield      #13  // Field hasCollision:Z
//!   4: ifeq          16
//!   7: aload_1
//!   8: aload_2
//!   9: aload_3
//!  10: invokevirtual #379 // Method BlockState.getShape:(LBlockGetter;LBlockPos;)LVoxelShape;
//!  13: goto          19
//!  16: invokestatic  #393 // Method Shapes.empty:()LVoxelShape;
//!  19: areturn
//! ```
//!
//! Format: `class|getShape_declaring_class|getCollisionShape_declaring_class`.
//! An empty second field means the class inherits `BlockBehaviour.getShape` directly (106 of 298
//! classes); an empty third field means the class inherits `BlockBehaviour.getCollisionShape`
//! directly (269 of 298 classes).

#[rustfmt::skip]
const TABLE: &str = r#"net/minecraft/world/level/block/AbstractBannerBlock||
net/minecraft/world/level/block/AbstractBedBlock||
net/minecraft/world/level/block/AbstractCandleBlock||
net/minecraft/world/level/block/AbstractCauldronBlock|net/minecraft/world/level/block/AbstractCauldronBlock|
net/minecraft/world/level/block/AbstractChestBlock||
net/minecraft/world/level/block/AbstractFurnaceBlock||
net/minecraft/world/level/block/AbstractSkullBlock||
net/minecraft/world/level/block/AirBlock|net/minecraft/world/level/block/AirBlock|
net/minecraft/world/level/block/AmethystBlock||
net/minecraft/world/level/block/AmethystClusterBlock|net/minecraft/world/level/block/AmethystClusterBlock|
net/minecraft/world/level/block/AnvilBlock|net/minecraft/world/level/block/AnvilBlock|
net/minecraft/world/level/block/AttachedStemBlock|net/minecraft/world/level/block/AttachedStemBlock|
net/minecraft/world/level/block/AzaleaBlock|net/minecraft/world/level/block/AzaleaBlock|
net/minecraft/world/level/block/BambooSaplingBlock|net/minecraft/world/level/block/BambooSaplingBlock|
net/minecraft/world/level/block/BambooStalkBlock|net/minecraft/world/level/block/BambooStalkBlock|net/minecraft/world/level/block/BambooStalkBlock
net/minecraft/world/level/block/BannerBlock|net/minecraft/world/level/block/BannerBlock|
net/minecraft/world/level/block/BarrelBlock||
net/minecraft/world/level/block/BarrierBlock||
net/minecraft/world/level/block/BaseCoralFanBlock|net/minecraft/world/level/block/BaseCoralFanBlock|
net/minecraft/world/level/block/BaseCoralPlantBlock|net/minecraft/world/level/block/BaseCoralPlantBlock|
net/minecraft/world/level/block/BaseCoralPlantTypeBlock|net/minecraft/world/level/block/BaseCoralPlantTypeBlock|
net/minecraft/world/level/block/BaseCoralWallFanBlock|net/minecraft/world/level/block/BaseCoralWallFanBlock|
net/minecraft/world/level/block/BaseEntityBlock||
net/minecraft/world/level/block/BaseFireBlock|net/minecraft/world/level/block/BaseFireBlock|
net/minecraft/world/level/block/BasePressurePlateBlock|net/minecraft/world/level/block/BasePressurePlateBlock|
net/minecraft/world/level/block/BaseRailBlock|net/minecraft/world/level/block/BaseRailBlock|
net/minecraft/world/level/block/BaseTorchBlock|net/minecraft/world/level/block/BaseTorchBlock|
net/minecraft/world/level/block/BeaconBlock||
net/minecraft/world/level/block/BedBlock|net/minecraft/world/level/block/BedBlock|
net/minecraft/world/level/block/BeehiveBlock||
net/minecraft/world/level/block/BeetrootBlock|net/minecraft/world/level/block/BeetrootBlock|
net/minecraft/world/level/block/BellBlock|net/minecraft/world/level/block/BellBlock|net/minecraft/world/level/block/BellBlock
net/minecraft/world/level/block/BigDripleafBlock|net/minecraft/world/level/block/BigDripleafBlock|net/minecraft/world/level/block/BigDripleafBlock
net/minecraft/world/level/block/BigDripleafStemBlock|net/minecraft/world/level/block/BigDripleafStemBlock|
net/minecraft/world/level/block/BlastFurnaceBlock||
net/minecraft/world/level/block/Block||
net/minecraft/world/level/block/BonemealableFeaturePlacerBlock||
net/minecraft/world/level/block/BrewingStandBlock|net/minecraft/world/level/block/BrewingStandBlock|
net/minecraft/world/level/block/BrushableBlock||
net/minecraft/world/level/block/BubbleColumnBlock|net/minecraft/world/level/block/BubbleColumnBlock|
net/minecraft/world/level/block/BuddingAmethystBlock||
net/minecraft/world/level/block/BushBlock|net/minecraft/world/level/block/BushBlock|
net/minecraft/world/level/block/ButtonBlock|net/minecraft/world/level/block/ButtonBlock|
net/minecraft/world/level/block/CactusBlock|net/minecraft/world/level/block/CactusBlock|net/minecraft/world/level/block/CactusBlock
net/minecraft/world/level/block/CactusFlowerBlock|net/minecraft/world/level/block/CactusFlowerBlock|
net/minecraft/world/level/block/CakeBlock|net/minecraft/world/level/block/CakeBlock|
net/minecraft/world/level/block/CalibratedSculkSensorBlock|net/minecraft/world/level/block/SculkSensorBlock|
net/minecraft/world/level/block/CampfireBlock|net/minecraft/world/level/block/CampfireBlock|
net/minecraft/world/level/block/CandleBlock|net/minecraft/world/level/block/CandleBlock|
net/minecraft/world/level/block/CandleCakeBlock|net/minecraft/world/level/block/CandleCakeBlock|
net/minecraft/world/level/block/CarpetBlock|net/minecraft/world/level/block/CarpetBlock|
net/minecraft/world/level/block/CarrotBlock|net/minecraft/world/level/block/CarrotBlock|
net/minecraft/world/level/block/CartographyTableBlock||
net/minecraft/world/level/block/CarvedPumpkinBlock||
net/minecraft/world/level/block/CauldronBlock|net/minecraft/world/level/block/AbstractCauldronBlock|
net/minecraft/world/level/block/CaveVinesBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/CaveVinesPlantBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/CeilingHangingSignBlock|net/minecraft/world/level/block/CeilingHangingSignBlock|
net/minecraft/world/level/block/ChainBlock|net/minecraft/world/level/block/ChainBlock|
net/minecraft/world/level/block/ChestBlock|net/minecraft/world/level/block/ChestBlock|
net/minecraft/world/level/block/ChiseledBookShelfBlock||
net/minecraft/world/level/block/ChorusFlowerBlock||
net/minecraft/world/level/block/ChorusPlantBlock|net/minecraft/world/level/block/PipeBlock|
net/minecraft/world/level/block/CocoaBlock|net/minecraft/world/level/block/CocoaBlock|
net/minecraft/world/level/block/ColoredFallingBlock||
net/minecraft/world/level/block/CommandBlock||
net/minecraft/world/level/block/ComparatorBlock|net/minecraft/world/level/block/DiodeBlock|
net/minecraft/world/level/block/ComposterBlock|net/minecraft/world/level/block/ComposterBlock|net/minecraft/world/level/block/ComposterBlock
net/minecraft/world/level/block/ConcretePowderBlock||
net/minecraft/world/level/block/ConduitBlock|net/minecraft/world/level/block/ConduitBlock|
net/minecraft/world/level/block/CopperBulbBlock||
net/minecraft/world/level/block/CopperChestBlock|net/minecraft/world/level/block/ChestBlock|
net/minecraft/world/level/block/CopperGolemStatueBlock|net/minecraft/world/level/block/CopperGolemStatueBlock|
net/minecraft/world/level/block/CoralBlock||
net/minecraft/world/level/block/CoralFanBlock|net/minecraft/world/level/block/BaseCoralFanBlock|
net/minecraft/world/level/block/CoralPlantBlock|net/minecraft/world/level/block/CoralPlantBlock|
net/minecraft/world/level/block/CoralWallFanBlock|net/minecraft/world/level/block/BaseCoralWallFanBlock|
net/minecraft/world/level/block/CrafterBlock||
net/minecraft/world/level/block/CraftingTableBlock||
net/minecraft/world/level/block/CreakingHeartBlock||
net/minecraft/world/level/block/CropBlock|net/minecraft/world/level/block/CropBlock|
net/minecraft/world/level/block/CrossCollisionBlock|net/minecraft/world/level/block/CrossCollisionBlock|net/minecraft/world/level/block/CrossCollisionBlock
net/minecraft/world/level/block/CryingObsidianBlock||
net/minecraft/world/level/block/DaylightDetectorBlock|net/minecraft/world/level/block/DaylightDetectorBlock|
net/minecraft/world/level/block/DecoratedPotBlock|net/minecraft/world/level/block/DecoratedPotBlock|
net/minecraft/world/level/block/DetectorRailBlock|net/minecraft/world/level/block/BaseRailBlock|
net/minecraft/world/level/block/DiodeBlock|net/minecraft/world/level/block/DiodeBlock|
net/minecraft/world/level/block/DirectionalBlock||
net/minecraft/world/level/block/DispenserBlock||
net/minecraft/world/level/block/DoorBlock|net/minecraft/world/level/block/DoorBlock|
net/minecraft/world/level/block/DoublePlantBlock||
net/minecraft/world/level/block/DragonEggBlock|net/minecraft/world/level/block/DragonEggBlock|
net/minecraft/world/level/block/DriedGhastBlock|net/minecraft/world/level/block/DriedGhastBlock|
net/minecraft/world/level/block/DropExperienceBlock||
net/minecraft/world/level/block/DropperBlock||
net/minecraft/world/level/block/DryVegetationBlock|net/minecraft/world/level/block/DryVegetationBlock|
net/minecraft/world/level/block/EnchantingTableBlock|net/minecraft/world/level/block/EnchantingTableBlock|
net/minecraft/world/level/block/EndGatewayBlock||
net/minecraft/world/level/block/EndPortalBlock|net/minecraft/world/level/block/EndPortalBlock|
net/minecraft/world/level/block/EndPortalFrameBlock|net/minecraft/world/level/block/EndPortalFrameBlock|
net/minecraft/world/level/block/EndRodBlock|net/minecraft/world/level/block/RodBlock|
net/minecraft/world/level/block/EnderChestBlock|net/minecraft/world/level/block/EnderChestBlock|
net/minecraft/world/level/block/EyeblossomBlock|net/minecraft/world/level/block/FlowerBlock|
net/minecraft/world/level/block/FaceAttachedHorizontalDirectionalBlock||
net/minecraft/world/level/block/FallingBlock||
net/minecraft/world/level/block/FallingParticlesLeavesBlock||
net/minecraft/world/level/block/FarmlandBlock|net/minecraft/world/level/block/FarmlandBlock|
net/minecraft/world/level/block/FenceBlock|net/minecraft/world/level/block/CrossCollisionBlock|net/minecraft/world/level/block/CrossCollisionBlock
net/minecraft/world/level/block/FenceGateBlock|net/minecraft/world/level/block/FenceGateBlock|net/minecraft/world/level/block/FenceGateBlock
net/minecraft/world/level/block/FireBlock|net/minecraft/world/level/block/FireBlock|
net/minecraft/world/level/block/FireflyBushBlock||
net/minecraft/world/level/block/FlowerBedBlock|net/minecraft/world/level/block/FlowerBedBlock|
net/minecraft/world/level/block/FlowerBlock|net/minecraft/world/level/block/FlowerBlock|
net/minecraft/world/level/block/FlowerPotBlock|net/minecraft/world/level/block/FlowerPotBlock|
net/minecraft/world/level/block/FrogspawnBlock|net/minecraft/world/level/block/FrogspawnBlock|
net/minecraft/world/level/block/FrostedIceBlock||
net/minecraft/world/level/block/FurnaceBlock||
net/minecraft/world/level/block/GlazedTerracottaBlock||
net/minecraft/world/level/block/GlowLichenBlock|net/minecraft/world/level/block/MultifaceBlock|
net/minecraft/world/level/block/GrassBlock||
net/minecraft/world/level/block/GrindstoneBlock|net/minecraft/world/level/block/GrindstoneBlock|net/minecraft/world/level/block/GrindstoneBlock
net/minecraft/world/level/block/GrowingPlantBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/GrowingPlantBodyBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/GrowingPlantHeadBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/HalfTransparentBlock||
net/minecraft/world/level/block/HangingMossBlock|net/minecraft/world/level/block/HangingMossBlock|
net/minecraft/world/level/block/HangingRootsBlock|net/minecraft/world/level/block/HangingRootsBlock|
net/minecraft/world/level/block/HayBlock||
net/minecraft/world/level/block/HeavyCoreBlock|net/minecraft/world/level/block/HeavyCoreBlock|
net/minecraft/world/level/block/HoneyBlock||net/minecraft/world/level/block/HoneyBlock
net/minecraft/world/level/block/HopperBlock|net/minecraft/world/level/block/HopperBlock|
net/minecraft/world/level/block/HorizontalDirectionalBlock||
net/minecraft/world/level/block/HugeMushroomBlock||
net/minecraft/world/level/block/IceBlock||
net/minecraft/world/level/block/InfestedBlock||
net/minecraft/world/level/block/InfestedRotatedPillarBlock||
net/minecraft/world/level/block/IronBarsBlock|net/minecraft/world/level/block/CrossCollisionBlock|net/minecraft/world/level/block/CrossCollisionBlock
net/minecraft/world/level/block/JigsawBlock||
net/minecraft/world/level/block/JukeboxBlock||
net/minecraft/world/level/block/KelpBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/KelpPlantBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/LadderBlock|net/minecraft/world/level/block/LadderBlock|
net/minecraft/world/level/block/LanternBlock|net/minecraft/world/level/block/LanternBlock|
net/minecraft/world/level/block/LavaCauldronBlock|net/minecraft/world/level/block/AbstractCauldronBlock|
net/minecraft/world/level/block/LayeredCauldronBlock|net/minecraft/world/level/block/AbstractCauldronBlock|
net/minecraft/world/level/block/LeafLitterBlock|net/minecraft/world/level/block/LeafLitterBlock|
net/minecraft/world/level/block/LeavesBlock||
net/minecraft/world/level/block/LecternBlock|net/minecraft/world/level/block/LecternBlock|net/minecraft/world/level/block/LecternBlock
net/minecraft/world/level/block/LeverBlock|net/minecraft/world/level/block/LeverBlock|
net/minecraft/world/level/block/LightBlock|net/minecraft/world/level/block/LightBlock|
net/minecraft/world/level/block/LightningRodBlock|net/minecraft/world/level/block/RodBlock|
net/minecraft/world/level/block/LilyPadBlock|net/minecraft/world/level/block/LilyPadBlock|
net/minecraft/world/level/block/LiquidBlock|net/minecraft/world/level/block/LiquidBlock|net/minecraft/world/level/block/LiquidBlock
net/minecraft/world/level/block/LoomBlock||
net/minecraft/world/level/block/MagmaBlock||
net/minecraft/world/level/block/MangroveLeavesBlock||
net/minecraft/world/level/block/MangrovePropaguleBlock|net/minecraft/world/level/block/MangrovePropaguleBlock|
net/minecraft/world/level/block/MangroveRootsBlock||
net/minecraft/world/level/block/MossyCarpetBlock|net/minecraft/world/level/block/MossyCarpetBlock|net/minecraft/world/level/block/MossyCarpetBlock
net/minecraft/world/level/block/MudBlock||net/minecraft/world/level/block/MudBlock
net/minecraft/world/level/block/MultifaceBlock|net/minecraft/world/level/block/MultifaceBlock|
net/minecraft/world/level/block/MultifaceSpreadeableBlock|net/minecraft/world/level/block/MultifaceBlock|
net/minecraft/world/level/block/MushroomBlock|net/minecraft/world/level/block/MushroomBlock|
net/minecraft/world/level/block/MyceliumBlock||
net/minecraft/world/level/block/NetherFungusBlock|net/minecraft/world/level/block/NetherFungusBlock|
net/minecraft/world/level/block/NetherPortalBlock|net/minecraft/world/level/block/NetherPortalBlock|
net/minecraft/world/level/block/NetherRootsBlock|net/minecraft/world/level/block/NetherRootsBlock|
net/minecraft/world/level/block/NetherSproutsBlock|net/minecraft/world/level/block/NetherSproutsBlock|
net/minecraft/world/level/block/NetherWartBlock|net/minecraft/world/level/block/NetherWartBlock|
net/minecraft/world/level/block/NetherrackBlock||
net/minecraft/world/level/block/NoteBlock||
net/minecraft/world/level/block/NyliumBlock||
net/minecraft/world/level/block/ObserverBlock||
net/minecraft/world/level/block/PathBlock|net/minecraft/world/level/block/PathBlock|
net/minecraft/world/level/block/PiglinWallSkullBlock|net/minecraft/world/level/block/PiglinWallSkullBlock|
net/minecraft/world/level/block/PipeBlock|net/minecraft/world/level/block/PipeBlock|
net/minecraft/world/level/block/PitcherCropBlock|net/minecraft/world/level/block/PitcherCropBlock|net/minecraft/world/level/block/PitcherCropBlock
net/minecraft/world/level/block/PlayerHeadBlock|net/minecraft/world/level/block/SkullBlock|net/minecraft/world/level/block/SkullBlock
net/minecraft/world/level/block/PlayerWallHeadBlock|net/minecraft/world/level/block/WallSkullBlock|
net/minecraft/world/level/block/PointedDripstoneBlock|net/minecraft/world/level/block/SpeleothemBlock|
net/minecraft/world/level/block/PotatoBlock|net/minecraft/world/level/block/PotatoBlock|
net/minecraft/world/level/block/PotentSulfurBlock||
net/minecraft/world/level/block/PowderSnowBlock||net/minecraft/world/level/block/PowderSnowBlock
net/minecraft/world/level/block/PoweredBlock||
net/minecraft/world/level/block/PoweredRailBlock|net/minecraft/world/level/block/BaseRailBlock|
net/minecraft/world/level/block/PressurePlateBlock|net/minecraft/world/level/block/BasePressurePlateBlock|
net/minecraft/world/level/block/PumpkinBlock||
net/minecraft/world/level/block/RailBlock|net/minecraft/world/level/block/BaseRailBlock|
net/minecraft/world/level/block/RedStoneOreBlock||
net/minecraft/world/level/block/RedstoneLampBlock||
net/minecraft/world/level/block/RedstoneTorchBlock|net/minecraft/world/level/block/BaseTorchBlock|
net/minecraft/world/level/block/RedstoneWallTorchBlock|net/minecraft/world/level/block/RedstoneWallTorchBlock|
net/minecraft/world/level/block/RedstoneWireBlock|net/minecraft/world/level/block/RedstoneWireBlock|
net/minecraft/world/level/block/RepeaterBlock|net/minecraft/world/level/block/DiodeBlock|
net/minecraft/world/level/block/RespawnAnchorBlock||
net/minecraft/world/level/block/RodBlock|net/minecraft/world/level/block/RodBlock|
net/minecraft/world/level/block/RootedDirtBlock||
net/minecraft/world/level/block/RotatedPillarBlock||
net/minecraft/world/level/block/SandBlock||
net/minecraft/world/level/block/SaplingBlock|net/minecraft/world/level/block/SaplingBlock|
net/minecraft/world/level/block/ScaffoldingBlock|net/minecraft/world/level/block/ScaffoldingBlock|net/minecraft/world/level/block/ScaffoldingBlock
net/minecraft/world/level/block/SculkBlock||
net/minecraft/world/level/block/SculkCatalystBlock||
net/minecraft/world/level/block/SculkSensorBlock|net/minecraft/world/level/block/SculkSensorBlock|
net/minecraft/world/level/block/SculkShriekerBlock||net/minecraft/world/level/block/SculkShriekerBlock
net/minecraft/world/level/block/SculkVeinBlock|net/minecraft/world/level/block/MultifaceBlock|
net/minecraft/world/level/block/SeaPickleBlock|net/minecraft/world/level/block/SeaPickleBlock|
net/minecraft/world/level/block/SeagrassBlock|net/minecraft/world/level/block/SeagrassBlock|
net/minecraft/world/level/block/ShelfBlock|net/minecraft/world/level/block/ShelfBlock|
net/minecraft/world/level/block/ShelfMushroomBlock|net/minecraft/world/level/block/ShelfMushroomBlock|
net/minecraft/world/level/block/ShortDryGrassBlock|net/minecraft/world/level/block/ShortDryGrassBlock|
net/minecraft/world/level/block/ShulkerBoxBlock|net/minecraft/world/level/block/ShulkerBoxBlock|
net/minecraft/world/level/block/SignBlock|net/minecraft/world/level/block/SignBlock|
net/minecraft/world/level/block/SkullBlock|net/minecraft/world/level/block/SkullBlock|net/minecraft/world/level/block/SkullBlock
net/minecraft/world/level/block/SlabBlock|net/minecraft/world/level/block/SlabBlock|
net/minecraft/world/level/block/SlimeBlock||
net/minecraft/world/level/block/SmallDripleafBlock|net/minecraft/world/level/block/SmallDripleafBlock|
net/minecraft/world/level/block/SmithingTableBlock||
net/minecraft/world/level/block/SmokerBlock||
net/minecraft/world/level/block/SnifferEggBlock|net/minecraft/world/level/block/SnifferEggBlock|
net/minecraft/world/level/block/SnowLayerBlock|net/minecraft/world/level/block/SnowLayerBlock|net/minecraft/world/level/block/SnowLayerBlock
net/minecraft/world/level/block/SnowyBlock||
net/minecraft/world/level/block/SoulFireBlock|net/minecraft/world/level/block/BaseFireBlock|
net/minecraft/world/level/block/SoulSandBlock||net/minecraft/world/level/block/SoulSandBlock
net/minecraft/world/level/block/SpawnerBlock||
net/minecraft/world/level/block/SpeleothemBlock|net/minecraft/world/level/block/SpeleothemBlock|
net/minecraft/world/level/block/SpongeBlock||
net/minecraft/world/level/block/SporeBlossomBlock|net/minecraft/world/level/block/SporeBlossomBlock|
net/minecraft/world/level/block/SpreadingSnowyBlock||
net/minecraft/world/level/block/StainedGlassBlock||
net/minecraft/world/level/block/StainedGlassPaneBlock|net/minecraft/world/level/block/CrossCollisionBlock|net/minecraft/world/level/block/CrossCollisionBlock
net/minecraft/world/level/block/StairBlock|net/minecraft/world/level/block/StairBlock|
net/minecraft/world/level/block/StandingSignBlock|net/minecraft/world/level/block/SignBlock|
net/minecraft/world/level/block/StemBlock|net/minecraft/world/level/block/StemBlock|
net/minecraft/world/level/block/StonecutterBlock|net/minecraft/world/level/block/StonecutterBlock|
net/minecraft/world/level/block/StrawBedBlock|net/minecraft/world/level/block/StrawBedBlock|
net/minecraft/world/level/block/StructureBlock||
net/minecraft/world/level/block/StructureVoidBlock|net/minecraft/world/level/block/StructureVoidBlock|
net/minecraft/world/level/block/SugarCaneBlock|net/minecraft/world/level/block/SugarCaneBlock|
net/minecraft/world/level/block/SulfurSpikeBlock|net/minecraft/world/level/block/SpeleothemBlock|
net/minecraft/world/level/block/SweetBerryBushBlock|net/minecraft/world/level/block/SweetBerryBushBlock|
net/minecraft/world/level/block/TallDryGrassBlock|net/minecraft/world/level/block/TallDryGrassBlock|
net/minecraft/world/level/block/TallFlowerBlock||
net/minecraft/world/level/block/TallGrassBlock|net/minecraft/world/level/block/TallGrassBlock|
net/minecraft/world/level/block/TallSeagrassBlock|net/minecraft/world/level/block/TallSeagrassBlock|
net/minecraft/world/level/block/TargetBlock||
net/minecraft/world/level/block/TestBlock||
net/minecraft/world/level/block/TestInstanceBlock||
net/minecraft/world/level/block/TintedGlassBlock||
net/minecraft/world/level/block/TintedParticleLeavesBlock||
net/minecraft/world/level/block/TntBlock||
net/minecraft/world/level/block/TorchBlock|net/minecraft/world/level/block/BaseTorchBlock|
net/minecraft/world/level/block/TorchflowerCropBlock|net/minecraft/world/level/block/TorchflowerCropBlock|
net/minecraft/world/level/block/TransparentBlock||
net/minecraft/world/level/block/TrapDoorBlock|net/minecraft/world/level/block/TrapDoorBlock|
net/minecraft/world/level/block/TrappedChestBlock|net/minecraft/world/level/block/ChestBlock|
net/minecraft/world/level/block/TrialSpawnerBlock||
net/minecraft/world/level/block/TripWireBlock|net/minecraft/world/level/block/TripWireBlock|
net/minecraft/world/level/block/TripWireHookBlock|net/minecraft/world/level/block/TripWireHookBlock|
net/minecraft/world/level/block/TurtleEggBlock|net/minecraft/world/level/block/TurtleEggBlock|
net/minecraft/world/level/block/TwistingVinesBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/TwistingVinesPlantBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/UntintedParticleLeavesBlock||
net/minecraft/world/level/block/VaultBlock||
net/minecraft/world/level/block/VegetationBlock||
net/minecraft/world/level/block/VineBlock|net/minecraft/world/level/block/VineBlock|
net/minecraft/world/level/block/WallBannerBlock|net/minecraft/world/level/block/WallBannerBlock|
net/minecraft/world/level/block/WallBlock|net/minecraft/world/level/block/WallBlock|net/minecraft/world/level/block/WallBlock
net/minecraft/world/level/block/WallHangingSignBlock|net/minecraft/world/level/block/WallHangingSignBlock|net/minecraft/world/level/block/WallHangingSignBlock
net/minecraft/world/level/block/WallSignBlock|net/minecraft/world/level/block/WallSignBlock|
net/minecraft/world/level/block/WallSkullBlock|net/minecraft/world/level/block/WallSkullBlock|
net/minecraft/world/level/block/WallTorchBlock|net/minecraft/world/level/block/WallTorchBlock|
net/minecraft/world/level/block/WaterloggedTransparentBlock||
net/minecraft/world/level/block/WeatheringCopperBarsBlock|net/minecraft/world/level/block/CrossCollisionBlock|net/minecraft/world/level/block/CrossCollisionBlock
net/minecraft/world/level/block/WeatheringCopperBulbBlock||
net/minecraft/world/level/block/WeatheringCopperChainBlock|net/minecraft/world/level/block/ChainBlock|
net/minecraft/world/level/block/WeatheringCopperChestBlock|net/minecraft/world/level/block/ChestBlock|
net/minecraft/world/level/block/WeatheringCopperDoorBlock|net/minecraft/world/level/block/DoorBlock|
net/minecraft/world/level/block/WeatheringCopperFullBlock||
net/minecraft/world/level/block/WeatheringCopperGolemStatueBlock|net/minecraft/world/level/block/CopperGolemStatueBlock|
net/minecraft/world/level/block/WeatheringCopperGrateBlock||
net/minecraft/world/level/block/WeatheringCopperSlabBlock|net/minecraft/world/level/block/SlabBlock|
net/minecraft/world/level/block/WeatheringCopperStairBlock|net/minecraft/world/level/block/StairBlock|
net/minecraft/world/level/block/WeatheringCopperTrapDoorBlock|net/minecraft/world/level/block/TrapDoorBlock|
net/minecraft/world/level/block/WeatheringLanternBlock|net/minecraft/world/level/block/LanternBlock|
net/minecraft/world/level/block/WeatheringLightningRodBlock|net/minecraft/world/level/block/RodBlock|
net/minecraft/world/level/block/WebBlock||
net/minecraft/world/level/block/WeepingVinesBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/WeepingVinesPlantBlock|net/minecraft/world/level/block/GrowingPlantBlock|
net/minecraft/world/level/block/WeightedPressurePlateBlock|net/minecraft/world/level/block/BasePressurePlateBlock|
net/minecraft/world/level/block/WetSpongeBlock||
net/minecraft/world/level/block/WitherRoseBlock|net/minecraft/world/level/block/FlowerBlock|
net/minecraft/world/level/block/WitherSkullBlock|net/minecraft/world/level/block/SkullBlock|net/minecraft/world/level/block/SkullBlock
net/minecraft/world/level/block/WitherWallSkullBlock|net/minecraft/world/level/block/WallSkullBlock|
net/minecraft/world/level/block/WoolCarpetBlock|net/minecraft/world/level/block/CarpetBlock|
net/minecraft/world/level/block/piston/MovingPistonBlock|net/minecraft/world/level/block/piston/MovingPistonBlock|net/minecraft/world/level/block/piston/MovingPistonBlock
net/minecraft/world/level/block/piston/PistonBaseBlock|net/minecraft/world/level/block/piston/PistonBaseBlock|
net/minecraft/world/level/block/piston/PistonHeadBlock|net/minecraft/world/level/block/piston/PistonHeadBlock|
"#;

fn row(class: &str) -> Option<(&'static str, &'static str)> {
    if class == "net/minecraft/world/level/block/state/BlockBehaviour" {
        return Some(("", ""));
    }
    if !class.starts_with("net/minecraft/world/level/block/") {
        return None;
    }
    static INDEX: std::sync::OnceLock<
        std::collections::HashMap<&'static str, (&'static str, &'static str)>,
    > = std::sync::OnceLock::new();
    INDEX
        .get_or_init(|| {
            TABLE
                .lines()
                .filter_map(|line| {
                    let mut parts = line.split('|');
                    let name = parts.next()?;
                    let shape_decl = parts.next()?;
                    let collision_decl = parts.next()?;
                    Some((name, (shape_decl, collision_decl)))
                })
                .collect()
        })
        .get(class)
        .copied()
}

/// Whether the vanilla block class `class` inherits `BlockBehaviour.getShape` directly
/// (`Shapes.block()`, the unit cube `[0, 0, 0, 1, 1, 1]`).
pub(crate) fn inherits_block_behaviour_get_shape(class: &str) -> bool {
    row(class).is_some_and(|(shape_decl, _)| shape_decl.is_empty())
}

/// Whether the vanilla block class `class` inherits `BlockBehaviour.getCollisionShape` directly
/// (`this.hasCollision ? state.getShape(level, pos) : Shapes.empty()`).
pub(crate) fn inherits_block_behaviour_get_collision_shape(class: &str) -> bool {
    row(class).is_some_and(|(_, collision_decl)| collision_decl.is_empty())
}

/// Whether `class` is a vanilla `Block` class that overrides `getShape` below `BlockBehaviour`.
#[cfg(test)]
pub(crate) fn overrides_get_shape(class: &str) -> bool {
    row(class).is_some_and(|(shape_decl, _)| !shape_decl.is_empty())
}

/// Whether `class` is a vanilla `Block` class that overrides `getCollisionShape` below
/// `BlockBehaviour`.
#[cfg(test)]
pub(crate) fn overrides_get_collision_shape(class: &str) -> bool {
    row(class).is_some_and(|(_, collision_decl)| !collision_decl.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_26_3_get_shape_table_counts_and_samples() {
        let mut total = 0;
        let mut shape_inheriting = 0;
        let mut shape_overriding = 0;
        let mut collision_inheriting = 0;
        let mut collision_overriding = 0;
        for line in TABLE.lines().filter(|line| !line.is_empty()) {
            let mut parts = line.split('|');
            let _class = parts.next().unwrap();
            let shape_decl = parts.next().unwrap();
            let collision_decl = parts.next().unwrap();
            assert!(parts.next().is_none());
            total += 1;
            if shape_decl.is_empty() {
                shape_inheriting += 1;
            } else {
                shape_overriding += 1;
            }
            if collision_decl.is_empty() {
                collision_inheriting += 1;
            } else {
                collision_overriding += 1;
            }
        }
        assert_eq!(total, 298);
        assert_eq!(shape_inheriting, 106);
        assert_eq!(shape_overriding, 192);
        assert_eq!(collision_inheriting, 269);
        assert_eq!(collision_overriding, 29);

        assert!(inherits_block_behaviour_get_shape(
            "net/minecraft/world/level/block/state/BlockBehaviour"
        ));
        assert!(inherits_block_behaviour_get_shape(
            "net/minecraft/world/level/block/Block"
        ));
        assert!(inherits_block_behaviour_get_shape(
            "net/minecraft/world/level/block/CraftingTableBlock"
        ));
        assert!(inherits_block_behaviour_get_shape(
            "net/minecraft/world/level/block/HorizontalDirectionalBlock"
        ));
        assert!(inherits_block_behaviour_get_collision_shape(
            "net/minecraft/world/level/block/Block"
        ));
        assert!(inherits_block_behaviour_get_collision_shape(
            "net/minecraft/world/level/block/CraftingTableBlock"
        ));

        assert!(overrides_get_shape(
            "net/minecraft/world/level/block/SlabBlock"
        ));
        assert!(overrides_get_shape(
            "net/minecraft/world/level/block/StairBlock"
        ));
        assert!(overrides_get_shape(
            "net/minecraft/world/level/block/EnchantingTableBlock"
        ));
        assert!(overrides_get_shape(
            "net/minecraft/world/level/block/AirBlock"
        ));
        assert!(overrides_get_collision_shape(
            "net/minecraft/world/level/block/FenceBlock"
        ));
        assert!(overrides_get_collision_shape(
            "net/minecraft/world/level/block/SoulSandBlock"
        ));
        assert!(overrides_get_collision_shape(
            "net/minecraft/world/level/block/HoneyBlock"
        ));
        assert!(!inherits_block_behaviour_get_shape(
            "net/minecraft/world/level/block/SlabBlock"
        ));
        assert!(!inherits_block_behaviour_get_collision_shape(
            "net/minecraft/world/level/block/FenceBlock"
        ));
        assert!(!inherits_block_behaviour_get_shape(
            "com/example/UnknownBlock"
        ));
        assert!(!inherits_block_behaviour_get_collision_shape(
            "com/example/UnknownBlock"
        ));
        assert!(!overrides_get_shape("com/example/UnknownBlock"));
        assert!(!overrides_get_collision_shape("com/example/UnknownBlock"));
    }
}
