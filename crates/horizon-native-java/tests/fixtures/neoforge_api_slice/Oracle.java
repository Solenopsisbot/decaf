import java.lang.reflect.Proxy;
import java.net.URL;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.ModLoadingContext;
import net.neoforged.neoforgespi.language.IConfigurable;
import net.neoforged.neoforgespi.language.IModFileInfo;
import net.neoforged.neoforgespi.language.IModInfo;
import net.neoforged.neoforgespi.language.IModLanguageLoader;
import net.neoforged.neoforgespi.locating.ForgeFeature;
import net.neoforged.neoforge.network.registration.HandlerThread;
import org.apache.maven.artifact.versioning.DefaultArtifactVersion;

public final class Oracle {
  private static IEventBus eventBus() {
    return (IEventBus) Proxy.newProxyInstance(
        Oracle.class.getClassLoader(), new Class<?>[] {IEventBus.class},
        (proxy, method, args) -> {
          Class<?> type = method.getReturnType();
          if (type == boolean.class) return false;
          if (type == byte.class) return (byte) 0;
          if (type == short.class) return (short) 0;
          if (type == int.class) return 0;
          if (type == long.class) return 0L;
          if (type == float.class) return 0.0f;
          if (type == double.class) return 0.0d;
          if (type == char.class) return '\0';
          return null;
        });
  }

  private static IModInfo modInfo() {
    return new IModInfo() {
      public IModFileInfo getOwningFile() { return null; }
      public IModLanguageLoader getLoader() { return null; }
      public String getModId() { return "demo"; }
      public String getDisplayName() { return "Demo Display"; }
      public String getDescription() { return "Demo Description"; }
      public org.apache.maven.artifact.versioning.ArtifactVersion getVersion() {
        return new DefaultArtifactVersion("1.0");
      }
      public List<? extends IModInfo.ModVersion> getDependencies() { return List.of(); }
      public List<? extends ForgeFeature.Bound> getForgeFeatures() { return List.of(); }
      public String getNamespace() { return "demo"; }
      public Map<String, Object> getModProperties() { return Map.of("answer", "yes"); }
      public Optional<URL> getUpdateURL() { return Optional.empty(); }
      public Optional<URL> getModURL() { return Optional.empty(); }
      public Optional<String> getLogoFile() { return Optional.empty(); }
      public boolean getLogoBlur() { return false; }
      public IConfigurable getConfig() { return null; }
    };
  }

  public static void main(String[] args) {
    IEventBus bus = eventBus();
    ModContainer container = new ModContainer(modInfo()) {
      public IEventBus getEventBus() { return bus; }
    };
    ModLoadingContext context = ModLoadingContext.get();
    context.setActiveContainer(container);
    ModContainer active = context.getActiveContainer();
    System.out.println("contextIdentity=" + (context == ModLoadingContext.get()));
    System.out.println("activeModId=" + active.getModId());
    System.out.println("activeNamespace=" + active.getNamespace());
    System.out.println("activeEventBusIdentity=" + (active.getEventBus() == bus && active.getEventBus() == active.getEventBus()));
    System.out.println("activeInfoModId=" + active.getModInfo().getModId());
    System.out.println("activeInfoVersion=" + active.getModInfo().getVersion());
    net.neoforged.neoforge.network.registration.PayloadRegistrar registrar =
        new net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent().registrar("1");
    net.neoforged.neoforge.network.registration.PayloadRegistrar versioned = registrar.versioned("2");
    net.neoforged.neoforge.network.registration.PayloadRegistrar optional = registrar.optional();
    net.neoforged.neoforge.network.registration.PayloadRegistrar threaded = registrar.executesOn(HandlerThread.MAIN);
    boolean emptyVersionNoThrow;
    try {
      registrar.versioned("");
      emptyVersionNoThrow = true;
    } catch (Throwable error) {
      emptyVersionNoThrow = false;
    }
    String badCast;
    try {
      String ignored = (String) (Object) registrar;
      badCast = "no-error";
    } catch (ClassCastException error) {
      badCast = error.getMessage();
    }
    System.out.println("registrarClass=" + registrar.getClass().getName());
    System.out.println("registrarVersionedDistinct=" + (registrar != versioned));
    System.out.println("registrarOptionalDistinct=" + (registrar != optional));
    System.out.println("registrarExecutesOnDistinct=" + (registrar != threaded));
    System.out.println("registrarEmptyVersionNoThrow=" + emptyVersionNoThrow);
    System.out.println("registrarCast=" + (((Object) registrar) instanceof net.neoforged.neoforge.network.registration.PayloadRegistrar));
    System.out.println("registrarBadCast=" + badCast);
  }
}
