//! Generic client container-screen bridge.
//!
//! Screen classes remain jar-owned; this module drives the real 26.3 screen
//! lifecycle (`MenuScreens$ScreenConstructor.create` -> `Gui.setScreen` ->
//! `Screen.init(int, int)` -> virtual `init()` -> `AbstractContainerScreen.init()`
//! / `ItemCombinerScreen.init()` -> `subInit()` -> `menu.addSlotListener(this)`
//! -> `extractBackground`) while letting every mod override run as bytecode and
//! counting anything unmodelled.

use crate::{
    Host, MethodRef, NativeError, NativeJar, Runtime, StdHost, Value,
    client_surface::{DrawOp, GUI_TEXTURED_PIPELINE},
    jdk_call::Site,
    menu_models::{
        ABSTRACT_CONTAINER_MENU, ABSTRACT_CONTAINER_SCREEN, ITEM_COMBINER_MENU,
        ITEM_COMBINER_SCREEN, MENU_TYPE, SCREEN,
    },
};

pub use crate::menu_models::{ScreenLayout, ScreenSlot, ScreenSlotKind};

const EXTRACT_BG_DESC: &str = "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;IIF)V";
const EXTRACT_ERROR_DESC: &str = "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;II)V";
const EXTRACT_TRANSPARENT_BG_DESC: &str = "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;)V";
const SCREEN_CONSTRUCTOR: &str = "net/minecraft/client/gui/screens/MenuScreens$ScreenConstructor";
const SCREEN_CONSTRUCTOR_CREATE_DESC: &str = "(Lnet/minecraft/world/inventory/AbstractContainerMenu;Lnet/minecraft/world/entity/player/Inventory;Lnet/minecraft/network/chat/Component;)Lnet/minecraft/client/gui/screens/Screen;";
const BLIT_10_DESC: &str = "(Lcom/mojang/renderpearl/api/pipeline/RenderPipeline;Lnet/minecraft/resources/Identifier;IIFFIIII)V";

#[derive(Clone, Debug, PartialEq)]
pub struct ClientScreenLayout {
    pub layout: ScreenLayout,
}

impl<H: Host> Runtime<'_, H> {
    /// Invoke a `MenuScreens$ScreenConstructor` (a constructor method reference,
    /// a static or capturing lambda, a bound method reference, or a jar class
    /// implementing `MenuScreens$ScreenConstructor.create`).
    fn invoke_screen_constructor(
        &mut self,
        constructor: &Value,
        menu: Value,
        inventory: Value,
        title: Value,
    ) -> Result<Value, NativeError> {
        let args = [menu, inventory, title];
        if let Some(screen) = self.invoke_callback(constructor, &args)? {
            return Ok(screen);
        }
        if crate::jar_object::object_class(constructor).is_some() {
            return self.call_virtual(
                Site::Interface,
                SCREEN_CONSTRUCTOR,
                "create",
                SCREEN_CONSTRUCTOR_CREATE_DESC,
                constructor,
                &args,
            );
        }
        Err(NativeError::Metadata(
            "MenuScreens constructor is not a jar callback".into(),
        ))
    }

    /// Run `Gui.setScreen` offsets 234 (virtual `added()`) and 298-319
    /// (`screen.init(guiWidth, guiHeight)`).
    fn run_screen_init(
        &mut self,
        screen: &Value,
        gui_width: i32,
        gui_height: i32,
    ) -> Result<(), NativeError> {
        // 26.3 Gui.setScreen offset 234: screen.added() (Screen.added offset 0 is return).
        if let Some(owner) = self.jar_screen_override(screen, "added", "()V")? {
            let _ = self.invoke(&owner, "added", "()V", Some(screen.clone()), &[])?;
        }
        let _ = self.invoke_vanilla_screen_method(
            SCREEN,
            "init",
            "(II)V",
            screen,
            &[Value::Int(gui_width), Value::Int(gui_height)],
        )?;
        Ok(())
    }

    /// Run `Gui.setScreen`'s teardown of the previous screen (virtual `removed()`).
    fn run_screen_removed(&mut self, screen: &Value) -> Result<(), NativeError> {
        self.dispatch_screen_virtual(screen, "removed", "()V", &[])?;
        Ok(())
    }

    /// Run virtual `extractBackground(graphics, mouseX, mouseY, partialTick)` on `screen`.
    fn run_screen_extract_background(
        &mut self,
        screen: &Value,
        graphics: Value,
        mouse_x: i32,
        mouse_y: i32,
        partial_tick: f32,
    ) -> Result<(), NativeError> {
        let args = [
            graphics,
            Value::Int(mouse_x),
            Value::Int(mouse_y),
            Value::Float(f64::from(partial_tick)),
        ];
        self.dispatch_screen_virtual(screen, "extractBackground", EXTRACT_BG_DESC, &args)?;
        Ok(())
    }

    /// The jar class overriding `(name, desc)` on `screen`, if any.
    fn jar_screen_override(
        &self,
        screen: &Value,
        name: &str,
        desc: &str,
    ) -> Result<Option<String>, NativeError> {
        let probe = MethodRef {
            owner: SCREEN.into(),
            name: name.into(),
            descriptor: desc.into(),
        };
        self.jvm_selected_jar_body(&probe, 0xb6, Some(screen))
    }

    /// Virtual dispatch of a `Screen` method on `screen`: runs the jar override
    /// if present; otherwise dispatches to the screen's modelled vanilla base
    /// class, or counts it if the vanilla base is unmodelled.
    fn dispatch_screen_virtual(
        &mut self,
        screen: &Value,
        name: &str,
        desc: &str,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        if let Some(owner) = self.jar_screen_override(screen, name, desc)? {
            return self.invoke(&owner, name, desc, Some(screen.clone()), args);
        }
        let Some(class) = crate::jar_object::object_class(screen) else {
            let r = MethodRef {
                owner: SCREEN.into(),
                name: name.into(),
                descriptor: desc.into(),
            };
            return self.host.unknown_operand_call(&r);
        };
        let vanilla_base = self
            .vanilla_superclass(class)
            .unwrap_or_else(|| class.to_owned());
        if let Some(value) =
            self.invoke_vanilla_screen_method(&vanilla_base, name, desc, screen, args)?
        {
            return Ok(value);
        }
        let r = MethodRef {
            owner: vanilla_base,
            name: name.into(),
            descriptor: desc.into(),
        };
        self.host.unknown_operand_call(&r)
    }

    /// Execute one modelled vanilla method of `Screen`, `AbstractContainerScreen`,
    /// or `ItemCombinerScreen` (for both virtual dispatch when not overridden
    /// and `invokespecial` `super.*` calls from mod overrides).
    ///
    /// Returns `Ok(None)` when `vanilla_class` or `(name, desc)` is outside the
    /// modelled surface so the caller counts it as unknown.
    pub(super) fn invoke_vanilla_screen_method(
        &mut self,
        vanilla_class: &str,
        name: &str,
        desc: &str,
        screen: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !matches!(
            vanilla_class,
            SCREEN | ABSTRACT_CONTAINER_SCREEN | ITEM_COMBINER_SCREEN
        ) {
            return Ok(None);
        }
        match (name, desc) {
            // 26.3 Screen.init(int, int) (public final) offsets 0-72:
            //   0-7:   this.width = width; this.height = height;
            //   10-25: if (!this.initialized) { this.init(); this.setInitialFocus(); }
            //   28-29: else { this.repositionElements(); }
            //   32-34: this.initialized = true;
            //   37-72: triggerImmediateNarration / suppressNarration
            ("init", "(II)V") => {
                let [Value::Int(w), Value::Int(h)] = args else {
                    return Ok(None);
                };
                let width_ref = MethodRef {
                    owner: SCREEN.into(),
                    name: "width".into(),
                    descriptor: "I".into(),
                };
                let height_ref = MethodRef {
                    owner: SCREEN.into(),
                    name: "height".into(),
                    descriptor: "I".into(),
                };
                self.host
                    .menu_field_set(screen, &width_ref, &Value::Int(*w));
                self.host
                    .menu_field_set(screen, &height_ref, &Value::Int(*h));
                let initialized = self.host.menu_screen_initialized(screen).unwrap_or(false);
                if !initialized {
                    self.dispatch_screen_virtual(screen, "init", "()V", &[])?;
                    // Offset 21: Screen.setInitialFocus() offsets 0-10 checks
                    // minecraft.getLastInputType().isKeyboard(), which Gui.setScreen
                    // offsets 83-90 set to InputType.NONE (false). Run a jar override
                    // if present.
                    if let Some(owner) =
                        self.jar_screen_override(screen, "setInitialFocus", "()V")?
                    {
                        let _ = self.invoke(
                            &owner,
                            "setInitialFocus",
                            "()V",
                            Some(screen.clone()),
                            &[],
                        )?;
                    }
                } else {
                    // Offset 29: virtual repositionElements() (a jar override, else
                    // Screen.repositionElements below).
                    self.dispatch_screen_virtual(screen, "repositionElements", "()V", &[])?;
                }
                self.host.set_menu_screen_initialized(screen, true);
                // Offset 39: virtual `triggerImmediateNarration(false)`.
                // 26.3 Screen.triggerImmediateNarration offsets 0-15 calls private
                // `shouldRunNarration()` (offsets 0-24: `DEBUG_UI_NARRATION ||
                // minecraft.getNarrator().isActive()`, false by default). Run any
                // jar override as bytecode; `super.triggerImmediateNarration` stays
                // unmodelled so calling super from an override is counted.
                if let Some(owner) =
                    self.jar_screen_override(screen, "triggerImmediateNarration", "(Z)V")?
                {
                    let _ = self.invoke(
                        &owner,
                        "triggerImmediateNarration",
                        "(Z)V",
                        Some(screen.clone()),
                        &[Value::Int(0)],
                    )?;
                }
                Ok(Some(Value::Void))
            }
            // 26.3 Screen.added() offset 0: return (allows `super.added()` from jar overrides).
            ("added", "()V") => Ok(Some(Value::Void)),
            // 26.3 Screen.resize(int, int) offsets 0-14 (`Minecraft.resizeDisplay`):
            //   0-7:   this.width = width; this.height = height;
            //   10-11: this.repositionElements()  (virtual)
            ("resize", "(II)V") => {
                let [Value::Int(w), Value::Int(h)] = args else {
                    return Ok(None);
                };
                for (name, value) in [("width", *w), ("height", *h)] {
                    self.host.menu_field_set(
                        screen,
                        &MethodRef {
                            owner: SCREEN.into(),
                            name: name.into(),
                            descriptor: "I".into(),
                        },
                        &Value::Int(value),
                    );
                }
                self.dispatch_screen_virtual(screen, "repositionElements", "()V", &[])?;
                Ok(Some(Value::Void))
            }
            // 26.3 Screen.repositionElements() offsets 0-4: this.rebuildWidgets() (virtual).
            // Neither AbstractContainerScreen nor ItemCombinerScreen overrides it.
            ("repositionElements", "()V") => {
                self.dispatch_screen_virtual(screen, "rebuildWidgets", "()V", &[])?;
                Ok(Some(Value::Void))
            }
            // 26.3 Screen.rebuildWidgets() offsets 0-16:
            //   0-1:   this.clearWidgets()   (offsets 0-27: renderables/children/narratables.clear())
            //   4-5:   this.clearFocus()     (offsets 0-16: getCurrentFocusPath(), applyFocus(false))
            //   8-9:   this.init()           (virtual)
            //   12-13: this.setInitialFocus()
            // Every widget or focus change a screen can make (`addRenderableWidget`,
            // `setFocused`, ...) is unmodelled and counted where it happens, so on an uncounted
            // run the three lists are empty and nothing is focused: clearWidgets and clearFocus
            // change nothing. A jar override of either (or of the focus reads clearFocus makes)
            // is not run here: it is counted.
            ("rebuildWidgets", "()V") => {
                for (name, desc) in [
                    ("clearWidgets", "()V"),
                    ("clearFocus", "()V"),
                    (
                        "getCurrentFocusPath",
                        "()Lnet/minecraft/client/gui/ComponentPath;",
                    ),
                    (
                        "getFocused",
                        "()Lnet/minecraft/client/gui/components/events/GuiEventListener;",
                    ),
                ] {
                    if let Some(owner) = self.jar_screen_override(screen, name, desc)? {
                        self.host.unknown_operand_call(&MethodRef {
                            owner,
                            name: name.into(),
                            descriptor: desc.into(),
                        })?;
                    }
                }
                self.dispatch_screen_virtual(screen, "init", "()V", &[])?;
                // As `init(int, int)` offset 21: Screen.setInitialFocus with no children
                // focuses nothing; a jar override runs.
                if let Some(owner) = self.jar_screen_override(screen, "setInitialFocus", "()V")? {
                    let _ =
                        self.invoke(&owner, "setInitialFocus", "()V", Some(screen.clone()), &[])?;
                }
                Ok(Some(Value::Void))
            }
            ("removed", "()V") => match vanilla_class {
                // 26.3 ItemCombinerScreen.removed() offsets 0-15:
                //   0-1:  super.removed()  (AbstractContainerScreen.removed)
                //   4-12: ((ItemCombinerMenu) this.menu).removeSlotListener(this)
                ITEM_COMBINER_SCREEN => {
                    self.invoke_vanilla_screen_method(
                        ABSTRACT_CONTAINER_SCREEN,
                        "removed",
                        "()V",
                        screen,
                        &[],
                    )?;
                    let menu_ref = MethodRef {
                        owner: ABSTRACT_CONTAINER_SCREEN.into(),
                        name: "menu".into(),
                        descriptor: "Lnet/minecraft/world/inventory/AbstractContainerMenu;".into(),
                    };
                    let menu = self
                        .host
                        .menu_field(screen, &menu_ref)
                        .unwrap_or(Value::Ref(None));
                    let remove_listener_ref = MethodRef {
                        owner: ITEM_COMBINER_MENU.into(),
                        name: "removeSlotListener".into(),
                        descriptor: "(Lnet/minecraft/world/inventory/ContainerListener;)V".into(),
                    };
                    if menu == Value::Ref(None) {
                        return Err(crate::null_checks::invoke_on_null(&remove_listener_ref));
                    }
                    if let Some(owner) =
                        self.jvm_selected_jar_body(&remove_listener_ref, 0xb6, Some(&menu))?
                    {
                        let _ = self.invoke(
                            &owner,
                            "removeSlotListener",
                            "(Lnet/minecraft/world/inventory/ContainerListener;)V",
                            Some(menu),
                            std::slice::from_ref(screen),
                        )?;
                    } else if self.host.menu_remove_slot_listener(&menu, screen).is_none() {
                        self.host.unknown_operand_call(&remove_listener_ref)?;
                    }
                    Ok(Some(Value::Void))
                }
                // 26.3 AbstractContainerScreen.removed() offsets 0-25:
                //   0-10:  if (this.minecraft.player == null) return;
                //   11-25: this.menu.removed(this.minecraft.player);
                ABSTRACT_CONTAINER_SCREEN => {
                    if !self.host.client_has_player() {
                        return Ok(Some(Value::Void));
                    }
                    let menu_ref = MethodRef {
                        owner: ABSTRACT_CONTAINER_SCREEN.into(),
                        name: "menu".into(),
                        descriptor: "Lnet/minecraft/world/inventory/AbstractContainerMenu;".into(),
                    };
                    let menu = self
                        .host
                        .menu_field(screen, &menu_ref)
                        .unwrap_or(Value::Ref(None));
                    let removed_ref = MethodRef {
                        owner: ABSTRACT_CONTAINER_MENU.into(),
                        name: "removed".into(),
                        descriptor: "(Lnet/minecraft/world/entity/player/Player;)V".into(),
                    };
                    if menu == Value::Ref(None) {
                        return Err(crate::null_checks::invoke_on_null(&removed_ref));
                    }
                    let player = crate::game_object::client_value("client/player".into());
                    self.call_virtual(
                        Site::Virtual,
                        ABSTRACT_CONTAINER_MENU,
                        "removed",
                        "(Lnet/minecraft/world/entity/player/Player;)V",
                        &menu,
                        &[player],
                    )?;
                    Ok(Some(Value::Void))
                }
                // 26.3 Screen.removed() offset 0: return
                SCREEN => Ok(Some(Value::Void)),
                _ => Ok(None),
            },
            ("init", "()V") => match vanilla_class {
                // 26.3 ItemCombinerScreen.init() offsets 0-19:
                //   0-1:  super.init()  (AbstractContainerScreen.init)
                //   4-5:  this.subInit()
                //   8-16: ((ItemCombinerMenu) this.menu).addSlotListener(this)
                ITEM_COMBINER_SCREEN => {
                    self.invoke_vanilla_screen_method(
                        ABSTRACT_CONTAINER_SCREEN,
                        "init",
                        "()V",
                        screen,
                        &[],
                    )?;
                    self.dispatch_screen_virtual(screen, "subInit", "()V", &[])?;
                    let menu_ref = MethodRef {
                        owner: ABSTRACT_CONTAINER_SCREEN.into(),
                        name: "menu".into(),
                        descriptor: "Lnet/minecraft/world/inventory/AbstractContainerMenu;".into(),
                    };
                    let menu = self
                        .host
                        .menu_field(screen, &menu_ref)
                        .unwrap_or(Value::Ref(None));
                    let add_listener_ref = MethodRef {
                        owner: ITEM_COMBINER_MENU.into(),
                        name: "addSlotListener".into(),
                        descriptor: "(Lnet/minecraft/world/inventory/ContainerListener;)V".into(),
                    };
                    if menu == Value::Ref(None) {
                        return Err(crate::null_checks::invoke_on_null(&add_listener_ref));
                    }
                    if let Some(owner) =
                        self.jvm_selected_jar_body(&add_listener_ref, 0xb6, Some(&menu))?
                    {
                        let _ = self.invoke(
                            &owner,
                            "addSlotListener",
                            "(Lnet/minecraft/world/inventory/ContainerListener;)V",
                            Some(menu),
                            std::slice::from_ref(screen),
                        )?;
                    } else {
                        let menu_vanilla = crate::jar_object::object_class(&menu)
                            .and_then(|c| self.vanilla_superclass(c))
                            .unwrap_or_else(|| ITEM_COMBINER_MENU.to_owned());
                        if matches!(
                            menu_vanilla.as_str(),
                            ABSTRACT_CONTAINER_MENU | ITEM_COMBINER_MENU
                        ) {
                            self.invoke_menu_add_slot_listener(&menu, screen)?;
                        } else {
                            self.host.unknown_operand_call(&MethodRef {
                                owner: menu_vanilla,
                                ..add_listener_ref
                            })?;
                        }
                    }
                    Ok(Some(Value::Void))
                }
                // 26.3 AbstractContainerScreen.init() offsets 0-54:
                //   0-12:  this.leftPos = (this.width - this.imageWidth) / 2
                //   15-27: this.topPos = (this.height - this.imageHeight) / 2
                //   30-34: this.itemSlotMouseActions.clear()
                //   39-51: this.addItemSlotMouseAction(new BundleMouseActions(this.minecraft))
                ABSTRACT_CONTAINER_SCREEN => {
                    let width = self.screen_int_field(screen, SCREEN, "width")?;
                    let height = self.screen_int_field(screen, SCREEN, "height")?;
                    let image_width =
                        self.screen_int_field(screen, ABSTRACT_CONTAINER_SCREEN, "imageWidth")?;
                    let image_height =
                        self.screen_int_field(screen, ABSTRACT_CONTAINER_SCREEN, "imageHeight")?;
                    let left_pos = width.wrapping_sub(image_width) / 2;
                    let top_pos = height.wrapping_sub(image_height) / 2;
                    self.host.menu_field_set(
                        screen,
                        &MethodRef {
                            owner: ABSTRACT_CONTAINER_SCREEN.into(),
                            name: "leftPos".into(),
                            descriptor: "I".into(),
                        },
                        &Value::Int(left_pos),
                    );
                    self.host.menu_field_set(
                        screen,
                        &MethodRef {
                            owner: ABSTRACT_CONTAINER_SCREEN.into(),
                            name: "topPos".into(),
                            descriptor: "I".into(),
                        },
                        &Value::Int(top_pos),
                    );
                    // Offsets 39-51: AbstractContainerScreen.addItemSlotMouseAction (offsets 0-11)
                    // only appends to private itemSlotMouseActions; if a mod overrides it, count it.
                    if let Some(owner) = self.jar_screen_override(
                        screen,
                        "addItemSlotMouseAction",
                        "(Lnet/minecraft/client/gui/ItemSlotMouseAction;)V",
                    )? {
                        self.host.unknown_operand_call(&MethodRef {
                            owner,
                            name: "addItemSlotMouseAction".into(),
                            descriptor: "(Lnet/minecraft/client/gui/ItemSlotMouseAction;)V".into(),
                        })?;
                    }
                    Ok(Some(Value::Void))
                }
                // 26.3 Screen.init() offset 0: return
                SCREEN => Ok(Some(Value::Void)),
                _ => Ok(None),
            },
            // 26.3 ItemCombinerScreen.subInit() offset 0: return
            ("subInit", "()V") if vanilla_class == ITEM_COMBINER_SCREEN => Ok(Some(Value::Void)),
            ("extractBackground", EXTRACT_BG_DESC) => match vanilla_class {
                // 26.3 ItemCombinerScreen.extractBackground offsets 0-57:
                //   0-6:   super.extractBackground(graphics, mouseX, mouseY, partialTick)
                //   9-41:  graphics.blit(RenderPipelines.GUI_TEXTURED, this.menuResource,
                //                        this.leftPos, this.topPos, 0.0f, 0.0f,
                //                        this.imageWidth, this.imageHeight, 256, 256)
                //   44-54: this.extractErrorIcon(graphics, this.leftPos, this.topPos)
                ITEM_COMBINER_SCREEN => {
                    self.invoke_vanilla_screen_method(
                        ABSTRACT_CONTAINER_SCREEN,
                        "extractBackground",
                        EXTRACT_BG_DESC,
                        screen,
                        args,
                    )?;
                    let Some(graphics) = args.first() else {
                        return Ok(None);
                    };
                    let menu_resource = self
                        .host
                        .menu_field(
                            screen,
                            &MethodRef {
                                owner: ITEM_COMBINER_SCREEN.into(),
                                name: "menuResource".into(),
                                descriptor: "Lnet/minecraft/resources/Identifier;".into(),
                            },
                        )
                        .unwrap_or(Value::Ref(None));
                    let left_pos =
                        self.screen_int_field(screen, ABSTRACT_CONTAINER_SCREEN, "leftPos")?;
                    let top_pos =
                        self.screen_int_field(screen, ABSTRACT_CONTAINER_SCREEN, "topPos")?;
                    let image_width =
                        self.screen_int_field(screen, ABSTRACT_CONTAINER_SCREEN, "imageWidth")?;
                    let image_height =
                        self.screen_int_field(screen, ABSTRACT_CONTAINER_SCREEN, "imageHeight")?;
                    let blit_ref = MethodRef {
                        owner: "net/minecraft/client/gui/GuiGraphicsExtractor".into(),
                        name: "blit".into(),
                        descriptor: BLIT_10_DESC.into(),
                    };
                    if *graphics == Value::Ref(None) {
                        return Err(crate::null_checks::invoke_on_null(&blit_ref));
                    }
                    let pipeline =
                        crate::game_object::client_value(GUI_TEXTURED_PIPELINE.to_owned());
                    self.host.invoke_instance(
                        &blit_ref,
                        graphics,
                        &[
                            pipeline,
                            menu_resource,
                            Value::Int(left_pos),
                            Value::Int(top_pos),
                            Value::Float(0.0),
                            Value::Float(0.0),
                            Value::Int(image_width),
                            Value::Int(image_height),
                            Value::Int(256),
                            Value::Int(256),
                        ],
                    )?;
                    // Offsets 44-54: re-read leftPos/topPos and call virtual extractErrorIcon.
                    let error_left =
                        self.screen_int_field(screen, ABSTRACT_CONTAINER_SCREEN, "leftPos")?;
                    let error_top =
                        self.screen_int_field(screen, ABSTRACT_CONTAINER_SCREEN, "topPos")?;
                    self.dispatch_screen_virtual(
                        screen,
                        "extractErrorIcon",
                        EXTRACT_ERROR_DESC,
                        &[
                            graphics.clone(),
                            Value::Int(error_left),
                            Value::Int(error_top),
                        ],
                    )?;
                    Ok(Some(Value::Void))
                }
                // 26.3 Screen.extractBackground offsets 0-55 + AbstractContainerScreen.isInGameUi offsets 0-1:
                //   0-12:  if (this.isInGameUi()) this.extractTransparentBackground(graphics)
                //   15-41: else panorama / blurred / menu background (not host-owned)
                //   42-55: minecraft.gui.hud.extractDeferredSubtitles()
                // The host backdrop boundary holds iff this is an AbstractContainerScreen
                // (isInGameUi() == true) and the mod overrides neither isInGameUi() nor
                // extractTransparentBackground(GuiGraphicsExtractor).
                ABSTRACT_CONTAINER_SCREEN | SCREEN => {
                    if !self.host.menu_screen_is_container(screen) {
                        return Ok(None);
                    }
                    if let Some(owner) = self.jar_screen_override(screen, "isInGameUi", "()Z")? {
                        self.host.unknown_operand_call(&MethodRef {
                            owner,
                            name: "isInGameUi".into(),
                            descriptor: "()Z".into(),
                        })?;
                        return Ok(Some(Value::Void));
                    }
                    if let Some(owner) = self.jar_screen_override(
                        screen,
                        "extractTransparentBackground",
                        EXTRACT_TRANSPARENT_BG_DESC,
                    )? {
                        self.host.unknown_operand_call(&MethodRef {
                            owner,
                            name: "extractTransparentBackground".into(),
                            descriptor: EXTRACT_TRANSPARENT_BG_DESC.into(),
                        })?;
                        return Ok(Some(Value::Void));
                    }
                    // The host's backdrop stands for this body: tell the screen bridge it ran.
                    self.host.note_screen_backdrop();
                    Ok(Some(Value::Void))
                }
                _ => Ok(None),
            },
            // 26.3 ItemCombinerScreen.slotChanged (offset 0: return) and dataChanged (offset 0: return)
            (
                "slotChanged",
                "(Lnet/minecraft/world/inventory/AbstractContainerMenu;ILnet/minecraft/world/item/ItemStack;)V",
            )
            | ("dataChanged", "(Lnet/minecraft/world/inventory/AbstractContainerMenu;II)V")
                if vanilla_class == ITEM_COMBINER_SCREEN =>
            {
                Ok(Some(Value::Void))
            }
            _ => Ok(None),
        }
    }

    /// The choke point for a container screen's frame ([`ClientSnapshot::screen_scope`]): any
    /// call on a modelled menu, slot list, slot, container or inventory that reads contents is
    /// counted, never answered from Decaf's copy. Sunrise never puts the client's slot contents
    /// into Java (it draws the items itself from the `MenuScreenLayout`), so no content read in
    /// that frame can be exact: not of the screen's own menu, not of a container held anywhere
    /// else (a `static`, a list, another object). A container only client code fills is then
    /// over-counted, which is sound. An allow-list keeps what observes no contents: construction
    /// and adding slots, the menu's type, slot lookup, data writes (`setData`, the sync itself),
    /// listeners, `removed`, `broadcastChanges` (`screen_menu_broadcast_unobservable` decides),
    /// and a slot's or container's shape. Data slots are not covered: the bridge writes the
    /// server's values into them (`sync_client_menu_data`).
    ///
    /// [`ClientSnapshot::screen_scope`]: crate::client_surface::ClientSnapshot::screen_scope
    #[inline(never)]
    pub(crate) fn screen_menu_unsynced_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
    ) -> Result<Option<Value>, NativeError> {
        let Some(host) = self.host.as_std_host() else {
            return Ok(None);
        };
        if !host.client_screen_scope() {
            return Ok(None);
        }
        let Some(key) = crate::expect_ref(receiver) else {
            return Ok(None);
        };
        if !matches!(
            host.menu_states.get(key),
            Some(
                crate::menu_models::Object::Menu(_)
                    | crate::menu_models::Object::SlotList { .. }
                    | crate::menu_models::Object::Slot(_)
                    | crate::menu_models::Object::Container { .. }
                    | crate::menu_models::Object::Inventory { .. }
            )
        ) {
            return Ok(None);
        }
        let shape = matches!(
            (r.name.as_str(), r.descriptor.as_str()),
            ("<init>", _)
                | ("getType", "()Lnet/minecraft/world/inventory/MenuType;")
                | ("getSlot", "(I)Lnet/minecraft/world/inventory/Slot;")
                | (
                    "addSlot",
                    "(Lnet/minecraft/world/inventory/Slot;)Lnet/minecraft/world/inventory/Slot;"
                )
                | (
                    "addDataSlot",
                    "(Lnet/minecraft/world/inventory/DataSlot;)Lnet/minecraft/world/inventory/DataSlot;"
                )
                | (
                    "addDataSlots",
                    "(Lnet/minecraft/world/inventory/ContainerData;)V"
                )
                | (
                    "addStandardInventorySlots"
                        | "addInventoryExtendedSlots"
                        | "addInventoryHotbarSlots",
                    "(Lnet/minecraft/world/Container;II)V"
                )
                | ("isValidSlotIndex", "(I)Z")
                | ("setData", "(II)V")
                | (
                    "addSlotListener" | "removeSlotListener",
                    "(Lnet/minecraft/world/inventory/ContainerListener;)V"
                )
                | ("removed", "(Lnet/minecraft/world/entity/player/Player;)V")
                | ("broadcastChanges", "()V")
                | ("isActive" | "isFake" | "isHighlightable", "()Z")
                | ("getNoItemIcon", "()Lnet/minecraft/resources/Identifier;")
                | ("getContainerSlot" | "getContainerSize", "()I")
                | ("hashCode", "()I")
                | ("equals", "(Ljava/lang/Object;)Z")
        );
        if shape {
            return Ok(None);
        }
        self.host.unknown_operand_call(r).map(Some)
    }

    /// 26.3 `AbstractContainerMenu.broadcastChanges` on a screen frame's client menu, when
    /// nothing can observe the stacks it reads: the client has no synchronizer (`setSynchronizer`
    /// is `ServerPlayer.initMenu`'s), every listener's `slotChanged`/`dataChanged` is
    /// `ItemCombinerScreen`'s (offset 0: `return`), and no jar `Slot.getItem`, container,
    /// `getCarried` or `DataSlot` body would run. Then the only effect is `lastSlots`, which
    /// becomes the copies of contents Decaf does not have ([`LastSlot::Unknown`]: a later
    /// comparison counts). `false` when any of that is not so (the modelled body runs, or counts).
    #[inline(never)]
    pub(crate) fn screen_menu_broadcast_unobservable(
        &mut self,
        menu: &Value,
    ) -> Result<bool, NativeError> {
        let Some(host) = self.host.as_std_host() else {
            return Ok(false);
        };
        if !host.client_screen_scope() {
            return Ok(false);
        }
        let Some(key) = crate::expect_ref(menu) else {
            return Ok(false);
        };
        let Some(crate::menu_models::Object::Menu(state)) = host.menu_states.get(key) else {
            return Ok(false);
        };
        let data_slots: Vec<Value> = state
            .live
            .data_slots
            .iter()
            .map(|entry| entry.slot.clone())
            .collect();
        let Some(entries) = self.host.menu_broadcast_slots(menu) else {
            return Ok(false);
        };
        let listeners = self.host.menu_listeners(menu);
        let jar = |value: &Value, runtime: &Self| {
            crate::jar_object::object_class(value)
                .is_some_and(|class| runtime.class(class).is_some())
        };
        for entry in &entries {
            if jar(&entry.slot, self) || jar(&entry.container, self) {
                return Ok(false);
            }
        }
        for slot in &data_slots {
            if jar(slot, self) {
                return Ok(false);
            }
            if let Some(slot_key) = crate::expect_ref(slot)
                && let Some(host) = self.host.as_std_host()
                && let Some(crate::menu_models::DataSlotKind::Container { .. }) =
                    crate::menu_models::data_slot_kind(&host.menu_states, slot_key)
            {
                // `DataSlot.forContainer`: its `get` is the `ContainerData`'s, usually a jar's.
                return Ok(false);
            }
        }
        let carried = MethodRef {
            owner: ABSTRACT_CONTAINER_MENU.into(),
            name: "getCarried".into(),
            descriptor: "()Lnet/minecraft/world/item/ItemStack;".into(),
        };
        if self
            .jvm_selected_jar_body(&carried, 0xb6, Some(menu))?
            .is_some()
        {
            return Ok(false);
        }
        for listener in &listeners {
            let vanilla = crate::jar_object::object_class(listener)
                .and_then(|class| self.vanilla_superclass(class));
            if vanilla.as_deref() != Some(ITEM_COMBINER_SCREEN) {
                return Ok(false);
            }
            for (name, desc) in [
                (
                    "slotChanged",
                    "(Lnet/minecraft/world/inventory/AbstractContainerMenu;ILnet/minecraft/world/item/ItemStack;)V",
                ),
                (
                    "dataChanged",
                    "(Lnet/minecraft/world/inventory/AbstractContainerMenu;II)V",
                ),
            ] {
                let r = MethodRef {
                    owner: "net/minecraft/world/inventory/ContainerListener".into(),
                    name: name.into(),
                    descriptor: desc.into(),
                };
                if self
                    .jvm_selected_jar_body(&r, 0xb9, Some(listener))?
                    .is_some()
                {
                    return Ok(false);
                }
            }
        }
        for entry in &entries {
            self.host
                .menu_set_last_slot(menu, entry.index, crate::menu_models::LastSlot::Unknown);
        }
        Ok(true)
    }

    fn screen_int_field(
        &mut self,
        screen: &Value,
        owner: &str,
        name: &str,
    ) -> Result<i32, NativeError> {
        let r = MethodRef {
            owner: owner.into(),
            name: name.into(),
            descriptor: "I".into(),
        };
        match self.host.menu_field(screen, &r) {
            Some(Value::Int(v)) => Ok(v),
            _ => Err(NativeError::Metadata(format!(
                "missing screen int field {owner}#{name}"
            ))),
        }
    }
}

/// The 26.3 `Screen` / `AbstractContainerScreen` / `ItemCombinerScreen` members a Sunrise mod
/// menu screen performs itself, not by running the Java screen: slot and item drawing, the
/// carried stack, labels, tooltips, every mouse and key input, ticking, the pause flag, the
/// close key, and the `ItemCombinerScreen` slot listener (the client menu holds no synced
/// contents). Sunrise does what the vanilla body does (`mod/src/menu_screen.rs` checks each
/// against its own container screen). A jar override of one would run in vanilla and is not
/// run here: [`NativeJar::count_sunrise_screen_overrides`] counts it.
pub const SUNRISE_SCREEN_METHODS: &[(&str, &str)] = &[
    (
        "extractRenderState",
        "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;IIF)V",
    ),
    (
        "extractContents",
        "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;IIF)V",
    ),
    (
        "extractCarriedItem",
        "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;II)V",
    ),
    (
        "extractSlots",
        "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;II)V",
    ),
    (
        "extractSlot",
        "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;Lnet/minecraft/world/inventory/Slot;II)V",
    ),
    (
        "extractTooltip",
        "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;II)V",
    ),
    (
        "getTooltipFromContainerItem",
        "(Lnet/minecraft/world/item/ItemStack;)Ljava/util/List;",
    ),
    (
        "extractLabels",
        "(Lnet/minecraft/client/gui/GuiGraphicsExtractor;II)V",
    ),
    (
        "mouseClicked",
        "(Lnet/minecraft/client/input/MouseButtonEvent;Z)Z",
    ),
    (
        "mouseReleased",
        "(Lnet/minecraft/client/input/MouseButtonEvent;)Z",
    ),
    (
        "mouseDragged",
        "(Lnet/minecraft/client/input/MouseButtonEvent;DD)Z",
    ),
    ("mouseScrolled", "(DDDD)Z"),
    ("mouseMoved", "(DD)V"),
    ("hasClickedOutside", "(DDII)Z"),
    ("isHovering", "(IIIIDD)Z"),
    (
        "slotClicked",
        "(Lnet/minecraft/world/inventory/Slot;IILnet/minecraft/world/inventory/ContainerInput;)V",
    ),
    (
        "onMouseClickAction",
        "(Lnet/minecraft/world/inventory/Slot;Lnet/minecraft/world/inventory/ContainerInput;)V",
    ),
    ("handleSlotStateChanged", "(IIZ)V"),
    ("keyPressed", "(Lnet/minecraft/client/input/KeyEvent;)Z"),
    ("keyReleased", "(Lnet/minecraft/client/input/KeyEvent;)Z"),
    (
        "charTyped",
        "(Lnet/minecraft/client/input/CharacterEvent;)Z",
    ),
    (
        "checkHotbarKeyPressed",
        "(Lnet/minecraft/client/input/KeyEvent;)Z",
    ),
    ("containerTick", "()V"),
    ("isPauseScreen", "()Z"),
    ("onClose", "()V"),
    (
        "slotChanged",
        "(Lnet/minecraft/world/inventory/AbstractContainerMenu;ILnet/minecraft/world/item/ItemStack;)V",
    ),
    (
        "dataChanged",
        "(Lnet/minecraft/world/inventory/AbstractContainerMenu;II)V",
    ),
];

/// A component of vanilla's text-component JSON (`{"text":..}`, `{"translate":..}`, a bare
/// string), as `Component` objects are made: a screen title the embedder hands over.
/// `None` when the JSON does not parse.
pub fn component_from_json(json: &str) -> Option<Value> {
    let parsed: serde_json::Value = serde_json::from_str(json).ok()?;
    let object =
        match parsed {
            serde_json::Value::String(text) => serde_json::Value::Object(
                serde_json::Map::from_iter([("text".to_owned(), serde_json::Value::String(text))]),
            ),
            object @ serde_json::Value::Object(_) => object,
            _ => return None,
        };
    Some(crate::host_component::component(&object))
}

impl NativeJar {
    /// Construct a client-side menu through `MenuType.create(containerId, inventory)`
    /// (26.3 `MenuScreens$ScreenConstructor.fromPacket` offsets 0-11).
    pub fn create_client_menu(
        &self,
        host: &mut StdHost,
        menu_type: Value,
        container_id: i32,
    ) -> Result<Value, NativeError> {
        if matches!(menu_type, Value::Ref(None)) {
            return Err(NativeError::Metadata("menu type is null".into()));
        }
        let inventory = host.client_inventory_value();
        let screen_scope = host.client_screen_scope();
        let menu = self.with_persistent_runtime(
            host,
            &[menu_type.clone(), inventory.clone()],
            |runtime| {
                let r = MethodRef {
                    owner: MENU_TYPE.into(),
                    name: "create".into(),
                    descriptor: "(ILnet/minecraft/world/entity/player/Inventory;)Lnet/minecraft/world/inventory/AbstractContainerMenu;".into(),
                };
                runtime
                    .invoke_menu_model(&r, &menu_type, &[Value::Int(container_id), inventory])?
                    .ok_or_else(|| {
                        NativeError::Metadata(
                            "MenuType was not constructed with a MenuSupplier".into(),
                        )
                    })
            },
        )?;
        // A new menu starts with none of the server's data values written into it.
        if screen_scope {
            host.client.screen_menu_data.clear();
        }
        Ok(menu)
    }

    /// Construct a jar object (allocating a real `JarObject` handle) and invoke
    /// its constructor with `args` under persistent class state and resident heap.
    pub fn instantiate_jar_object(
        &self,
        class: &str,
        descriptor: &str,
        args: &[Value],
        host: &mut StdHost,
    ) -> Result<Value, NativeError> {
        let class_name = class.replace('.', "/");
        self.with_persistent_runtime(host, args, |runtime| {
            let object = runtime.new_object(&class_name);
            runtime.invoke(
                &class_name,
                "<init>",
                descriptor,
                Some(object.clone()),
                args,
            )?;
            Ok(object)
        })
    }

    /// Construct and initialize (`Screen.init(guiWidth, guiHeight)`) the screen
    /// registered for `menu_type`, tearing down any previously active screen via
    /// virtual `removed()`, and return its post-init layout.
    pub fn open_client_screen(
        &self,
        host: &mut StdHost,
        menu_type: Value,
        title: Value,
    ) -> Result<ClientScreenLayout, NativeError> {
        let [gui_w, gui_h] = host
            .client_gui_size()
            .ok_or_else(|| NativeError::Metadata("client frame is not active".into()))?;
        let constructor = host.client_menu_constructor(&menu_type).ok_or_else(|| {
            NativeError::Metadata("no MenuScreens registration for menu type".into())
        })?;
        let menu = host
            .client_menu_value_for_type(&menu_type)
            .ok_or_else(|| NativeError::Metadata("no constructed menu matches menu type".into()))?;
        let inventory = host.client_inventory_value();
        let screen = self.switch_client_screen(
            host,
            ScreenSwitch {
                menu_type,
                menu,
                title,
                constructor,
                inventory,
                gui_size: [gui_w, gui_h],
                background: false,
            },
        )?;
        let layout = host.client_screen_layout(&screen).ok_or_else(|| {
            NativeError::Metadata("screen constructor did not initialize a container screen".into())
        })?;
        Ok(ClientScreenLayout { layout })
    }

    /// Invoke `extractBackground` on the active screen for `menu_type` (reusing
    /// an already-open screen from `open_client_screen` for the same menu/title/size
    /// so no duplicate screen or leaked slot listener is created; otherwise tearing
    /// down any previous screen via `removed()`, constructing, and initializing),
    /// returning the recorded [`DrawOp`]s.
    pub fn draw_client_screen_bg(
        &self,
        host: &mut StdHost,
        menu_type: Value,
        title: Value,
    ) -> Result<Vec<DrawOp>, NativeError> {
        let [gui_w, gui_h] = host
            .client_gui_size()
            .ok_or_else(|| NativeError::Metadata("client frame is not active".into()))?;
        let constructor = host.client_menu_constructor(&menu_type).ok_or_else(|| {
            NativeError::Metadata("no MenuScreens registration for menu type".into())
        })?;
        let menu = host
            .client_menu_value_for_type(&menu_type)
            .ok_or_else(|| NativeError::Metadata("no constructed menu matches menu type".into()))?;
        let inventory = host.client_inventory_value();
        let before = host.client_draws().len();
        if let Some(existing) =
            host.client_matching_active_screen(&menu_type, &menu, &title, [gui_w, gui_h])
        {
            let root = existing.clone();
            self.with_persistent_runtime(host, std::slice::from_ref(&root), |runtime| {
                runtime.run_screen_extract_background(
                    &existing,
                    crate::game_object::client_value("client/gui-graphics".to_owned()),
                    0,
                    0,
                    0.0,
                )?;
                Ok(existing)
            })?;
            return Ok(host.client_draws()[before..].to_vec());
        }
        self.switch_client_screen(
            host,
            ScreenSwitch {
                menu_type,
                menu,
                title,
                constructor,
                inventory,
                gui_size: [gui_w, gui_h],
                background: true,
            },
        )?;
        Ok(host.client_draws()[before..].to_vec())
    }

    /// `MenuScreens$ScreenConstructor.fromPacket` + `Gui.setScreen` for `switch`, with the
    /// active screen left as vanilla leaves it on every exit, an exception included, or an
    /// adaptation that is counted.
    ///
    /// 26.3 order: `fromPacket` constructs the new screen first (offset 22); if construction
    /// throws or returns null, the previous screen is untouched. Then `Gui.setScreen` runs the
    /// previous screen's `removed()` (offset 77), switches the screen (offset 220), and runs
    /// `added()` (234) and `init(w, h)` (298-319). Neither method, nor
    /// `ClientPacketListener.handleOpenScreen`, has an exception table, so an exception leaves
    /// `Gui.screen` as far as it got. `PacketProcessor$ListenerAndPacket.handle` then catches it
    /// and calls `PacketListener.onPacketError`, whose default throws a `ReportedException`: a
    /// client crash report. A construction that fails is the caller's to count; a `removed()`
    /// or `init` that throws is counted here (`Gui.setScreen`).
    fn switch_client_screen(
        &self,
        host: &mut StdHost,
        switch: ScreenSwitch,
    ) -> Result<Value, NativeError> {
        #[derive(Clone, Copy, PartialEq)]
        enum Stage {
            Construct,
            RemovePrevious,
            Switched,
        }
        let ScreenSwitch {
            menu_type,
            menu,
            title,
            constructor,
            inventory,
            gui_size,
            background,
        } = switch;
        let previous = host.client_active_screen_value();
        let mut roots = vec![
            menu_type.clone(),
            constructor.clone(),
            menu.clone(),
            inventory.clone(),
            title.clone(),
        ];
        if let Some(prev) = &previous {
            roots.push(prev.clone());
        }
        let mut stage = Stage::Construct;
        let result = self.with_persistent_runtime(host, &roots, |runtime| {
            let screen = runtime.invoke_screen_constructor(
                &constructor,
                menu.clone(),
                inventory,
                title.clone(),
            )?;
            if matches!(screen, Value::Ref(None)) {
                return Err(NativeError::Metadata(
                    "MenuScreens constructor returned null".into(),
                ));
            }
            stage = Stage::RemovePrevious;
            if let Some(prev) = previous {
                runtime.run_screen_removed(&prev)?;
            }
            runtime.host.set_client_active_screen_slot(
                menu_type.clone(),
                menu.clone(),
                title.clone(),
                gui_size,
                screen.clone(),
            );
            stage = Stage::Switched;
            runtime.run_screen_init(&screen, gui_size[0], gui_size[1])?;
            if background {
                runtime.run_screen_extract_background(
                    &screen,
                    crate::game_object::client_value("client/gui-graphics".to_owned()),
                    0,
                    0,
                    0.0,
                )?;
            }
            Ok(screen)
        });
        let set_screen = MethodRef {
            owner: "net/minecraft/client/gui/Gui".into(),
            name: "setScreen".into(),
            descriptor: "(Lnet/minecraft/client/gui/screens/Screen;)V".into(),
        };
        match (&result, stage) {
            // The new screen never showed: the previous screen stays as it was.
            (Err(_), Stage::Construct) => {}
            // The previous screen's `removed()` threw. Vanilla leaves that screen installed in
            // `Gui.screen`, its `removed()` already run, and the client then crashes (above).
            // Decaf does not crash the game: it switches the screen out (no screen is left to
            // draw), a deliberate adaptation, counted as `Gui.setScreen`.
            (Err(_), Stage::RemovePrevious) => {
                let _ = host.unknown_host_call(&set_screen);
                let _ = host.take_client_active_screen();
                host.clear_client_screen_menus();
            }
            // Switched, then `init` threw: `Gui.screen` is the new screen, as in vanilla
            // before its crash report; counted.
            (Err(_), Stage::Switched) => {
                let _ = host.unknown_host_call(&set_screen);
            }
            (Ok(_), _) => {}
        }
        result
    }
}

/// One screen switch's inputs (`NativeJar::switch_client_screen`).
struct ScreenSwitch {
    menu_type: Value,
    menu: Value,
    title: Value,
    constructor: Value,
    inventory: Value,
    gui_size: [i32; 2],
    background: bool,
}

impl NativeJar {
    /// The active client screen's layout as it is now (after `init` or a resize).
    pub fn active_client_screen_layout(&self, host: &StdHost) -> Option<ScreenLayout> {
        let screen = host.client_active_screen_value()?;
        host.client_screen_layout(&screen)
    }

    /// Count each [`SUNRISE_SCREEN_METHODS`] member the active screen's jar classes override:
    /// one counted call per member (`<jar class>#<name><descriptor>`), returned as the keys.
    pub fn count_sunrise_screen_overrides(
        &self,
        host: &mut StdHost,
    ) -> Result<Vec<String>, NativeError> {
        let screen = host
            .client_active_screen_value()
            .ok_or_else(|| NativeError::Metadata("no active client screen".into()))?;
        let mut counted = Vec::new();
        self.with_persistent_runtime(host, std::slice::from_ref(&screen), |runtime| {
            for (name, desc) in SUNRISE_SCREEN_METHODS {
                if let Some(owner) = runtime.jar_screen_override(&screen, name, desc)? {
                    let r = MethodRef {
                        owner,
                        name: (*name).into(),
                        descriptor: (*desc).into(),
                    };
                    // Counted, not run: the run goes on.
                    let _ = runtime.host.unknown_operand_call(&r);
                    counted.push(format!("{}#{}{}", r.owner, r.name, r.descriptor));
                }
            }
            Ok(Value::Void)
        })?;
        Ok(counted)
    }

    /// One frame of the active client screen's own drawing: `Screen.resize(w, h)` first when
    /// the frame's GUI size differs from the one it was laid out for (`Minecraft.resizeDisplay`),
    /// then virtual `extractBackground(graphics, mouseX, mouseY, partialTick)`. Returns the
    /// recorded [`DrawOp`]s, also when the run stops at a counted call (the ops it drew so far).
    ///
    /// `partial_tick` is `None` when the embedder doesn't have it: then a jar
    /// `extractBackground` override (the only body here that receives it) counts
    /// `DeltaTracker.getGameTimeDeltaPartialTick` and runs with `0.0f`.
    pub fn draw_active_client_screen(
        &self,
        host: &mut StdHost,
        mouse: [i32; 2],
        partial_tick: Option<f32>,
    ) -> (Vec<DrawOp>, Result<(), NativeError>) {
        let before = host.client_draws().len();
        host.client_screen_backdrop_reset();
        let result = self.draw_active_client_screen_inner(host, mouse, partial_tick);
        (host.client_draws()[before..].to_vec(), result)
    }

    fn draw_active_client_screen_inner(
        &self,
        host: &mut StdHost,
        [mouse_x, mouse_y]: [i32; 2],
        partial_tick: Option<f32>,
    ) -> Result<(), NativeError> {
        let [gui_w, gui_h] = host
            .client_gui_size()
            .ok_or_else(|| NativeError::Metadata("client frame is not active".into()))?;
        let (menu_type, menu, title, size, screen) = host
            .client_active_screen()
            .ok_or_else(|| NativeError::Metadata("no active client screen".into()))?;
        let resize = size != [gui_w, gui_h];
        if resize {
            host.set_client_active_screen(menu_type, menu, title, [gui_w, gui_h], screen.clone());
        }
        self.with_persistent_runtime(host, std::slice::from_ref(&screen), |runtime| {
            if resize {
                runtime.dispatch_screen_virtual(
                    &screen,
                    "resize",
                    "(II)V",
                    &[Value::Int(gui_w), Value::Int(gui_h)],
                )?;
            }
            let partial = match partial_tick {
                Some(partial) => partial,
                None => {
                    if runtime
                        .jar_screen_override(&screen, "extractBackground", EXTRACT_BG_DESC)?
                        .is_some()
                    {
                        let _ = runtime.host.unknown_operand_call(&MethodRef {
                            owner: "net/minecraft/client/DeltaTracker".into(),
                            name: "getGameTimeDeltaPartialTick".into(),
                            descriptor: "(Z)F".into(),
                        });
                    }
                    0.0
                }
            };
            runtime.run_screen_extract_background(
                &screen,
                crate::game_object::client_value("client/gui-graphics".to_owned()),
                mouse_x,
                mouse_y,
                partial,
            )?;
            Ok(Value::Void)
        })?;
        Ok(())
    }

    /// Write the server's data-slot values Sunrise holds for the open menu (`MenuScreenFrame::data`)
    /// into the active screen's client menu, as vanilla's `ClientPacketListener
    /// .handleContainerSetData` does: virtual `menu.setData(id, value)` (26.3 offsets 0-17:
    /// `dataSlots.get(id).set(value)`). The packet carries a `short` (`ByteBufCodecs.SHORT`), so
    /// the client sees the value sign-extended from 16 bits. A value is written once per change,
    /// as the server sends one (`synchronizeDataSlotToRemote` compares `remoteDataSlots`); values
    /// Sunrise has not received yet stay as the menu built them, as on a vanilla client before
    /// the packet. An index past the menu's data slots (vanilla's `dataSlots.get` throws in the
    /// packet handler) counts `AbstractContainerMenu.setData`.
    pub fn sync_client_menu_data(
        &self,
        host: &mut StdHost,
        data: &[i32],
    ) -> Result<(), NativeError> {
        let Some((_, menu, _, _, _)) = host.client_active_screen() else {
            return Ok(());
        };
        let Some(key) = crate::expect_ref(&menu) else {
            return Ok(());
        };
        let slots = match host.menu_states.get(key) {
            Some(crate::menu_models::Object::Menu(state)) => state.live.data_slots.len(),
            _ => 0,
        };
        let set_data = MethodRef {
            owner: ABSTRACT_CONTAINER_MENU.into(),
            name: "setData".into(),
            descriptor: "(II)V".into(),
        };
        if data.len() > slots {
            let _ = host.unknown_host_call(&set_data);
        }
        let mut writes = Vec::new();
        for (index, value) in data.iter().take(slots).enumerate() {
            let value = i32::from(*value as i16);
            if host.client.screen_menu_data.get(index).copied().flatten() != Some(value) {
                writes.push((index, value));
            }
        }
        if writes.is_empty() {
            return Ok(());
        }
        self.with_persistent_runtime(host, std::slice::from_ref(&menu), |runtime| {
            for (index, value) in &writes {
                runtime.call_virtual(
                    Site::Virtual,
                    ABSTRACT_CONTAINER_MENU,
                    "setData",
                    "(II)V",
                    &menu,
                    &[Value::Int(*index as i32), Value::Int(*value)],
                )?;
            }
            Ok(Value::Void)
        })?;
        let synced = &mut host.client.screen_menu_data;
        for (index, value) in writes {
            if synced.len() <= index {
                synced.resize(index + 1, None);
            }
            synced[index] = Some(value);
        }
        Ok(())
    }

    /// Whether the active screen's `Screen.title` is the title it was opened with (the one
    /// Sunrise draws as the label): `false` when its constructor handed `super` another one,
    /// which vanilla's `extractLabels` would draw instead. `None` with no active screen.
    pub fn active_client_screen_keeps_its_title(&self, host: &mut StdHost) -> Option<bool> {
        let (_, _, title, _, screen) = host.client_active_screen()?;
        let field = MethodRef {
            owner: SCREEN.into(),
            name: "title".into(),
            descriptor: "Lnet/minecraft/network/chat/Component;".into(),
        };
        let own = crate::Host::menu_field(host, &screen, &field)?;
        Some(own == title)
    }

    /// Each slot of the active screen's menu, in order, as Sunrise lays it out at open:
    /// virtual `isActive()` and `getNoItemIcon()` (26.3 `Slot` bodies: `iconst_1; ireturn` and
    /// `aconst_null; areturn`; `ItemCombinerMenu$2`/`$3` override neither), a jar override run.
    /// Sunrise asks neither again, nor `isFake`, `isHighlightable` or `getMaxStackSize(ItemStack)`
    /// (vanilla `false`, `true`, `min(getMaxStackSize(), stack.getMaxStackSize())`), so a jar
    /// override of any of the five is counted. A run that counted keeps the vanilla answer.
    pub fn client_screen_slot_presentation(
        &self,
        host: &mut StdHost,
    ) -> Result<Vec<(bool, Option<String>)>, NativeError> {
        let Some((_, menu, _, _, _)) = host.client_active_screen() else {
            return Ok(Vec::new());
        };
        let Some(menu_key) = crate::expect_ref(&menu) else {
            return Ok(Vec::new());
        };
        let slots = match host.menu_states.get(menu_key) {
            Some(crate::menu_models::Object::Menu(state)) => {
                match host.menu_states.get(&state.slots) {
                    Some(crate::menu_models::Object::SlotList { slots }) => slots
                        .iter()
                        .map(|key| crate::menu_models::marker_of(key))
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        };
        let mut out = Vec::with_capacity(slots.len());
        let mut roots = slots.clone();
        roots.push(menu);
        self.with_persistent_runtime(host, &roots, |runtime| {
            for slot in &slots {
                let mut active = true;
                let mut icon = None;
                for (name, desc) in [
                    ("isActive", "()Z"),
                    ("getNoItemIcon", "()Lnet/minecraft/resources/Identifier;"),
                    ("isFake", "()Z"),
                    ("isHighlightable", "()Z"),
                    ("getMaxStackSize", "(Lnet/minecraft/world/item/ItemStack;)I"),
                ] {
                    let r = MethodRef {
                        owner: crate::menu_models::SLOT.into(),
                        name: name.into(),
                        descriptor: desc.into(),
                    };
                    let Some(owner) = runtime.jvm_selected_jar_body(&r, 0xb6, Some(slot))? else {
                        continue;
                    };
                    let _ = runtime.host.unknown_operand_call(&MethodRef {
                        owner: owner.clone(),
                        ..r.clone()
                    });
                    if !matches!(name, "isActive" | "getNoItemIcon") {
                        continue;
                    }
                    let before = runtime.host.counted_unknown_calls();
                    let value = runtime.invoke(&owner, name, desc, Some(slot.clone()), &[])?;
                    if runtime.host.counted_unknown_calls() != before {
                        continue;
                    }
                    match (name, value) {
                        ("isActive", Value::Int(flag)) => active = flag != 0,
                        ("getNoItemIcon", Value::Ref(None)) => icon = None,
                        ("getNoItemIcon", id) => {
                            icon = crate::host_ids::identifier_key(&id).map(str::to_owned)
                        }
                        _ => {}
                    }
                }
                out.push((active, icon));
            }
            Ok(Value::Void)
        })?;
        Ok(out)
    }

    /// Close the active client screen as `Gui.setScreen(null)` does (offset 77, virtual
    /// `removed()`: `ItemCombinerScreen` removes its slot listener, `AbstractContainerScreen`
    /// runs the client menu's `removed(player)`). False when no screen is active.
    pub fn close_active_client_screen(&self, host: &mut StdHost) -> Result<bool, NativeError> {
        let Some(screen) = host.client_active_screen_value() else {
            return Ok(false);
        };
        // `removed()` runs while the screen and its menu are still the client's (`Gui.setScreen`
        // offset 77, before the switch at 220): a read of the menu there is still counted.
        let result = self.with_persistent_runtime(host, std::slice::from_ref(&screen), |runtime| {
            runtime.run_screen_removed(&screen)?;
            Ok(Value::Void)
        });
        let _ = host.take_client_active_screen();
        host.clear_client_screen_menus();
        result?;
        Ok(true)
    }
}
