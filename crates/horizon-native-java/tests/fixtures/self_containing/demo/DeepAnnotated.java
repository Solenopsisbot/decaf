package demo;

import java.lang.annotation.*;

@Retention(RetentionPolicy.RUNTIME) @interface Deep0 { Deep1 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep1 { Deep2 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep2 { Deep3 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep3 { Deep4 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep4 { Deep5 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep5 { Deep6 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep6 { Deep7 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep7 { Deep8 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep8 { Deep9 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep9 { Deep10 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep10 { Deep11 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep11 { Deep12 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep12 { Deep13 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep13 { Deep14 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep14 { Deep15 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep15 { Deep16 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep16 { Deep17 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep17 { Deep18 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep18 { Deep19 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep19 { Deep20 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep20 { Deep21 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep21 { Deep22 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep22 { Deep23 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep23 { Deep24 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep24 { Deep25 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep25 { Deep26 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep26 { Deep27 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep27 { Deep28 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep28 { Deep29 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep29 { Deep30 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep30 { Deep31 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep31 { Deep32 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep32 { Deep33 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep33 { Deep34 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep34 { Deep35 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep35 { Deep36 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep36 { Deep37 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep37 { Deep38 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep38 { Deep39 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep39 { Deep40 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep40 { Deep41 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep41 { Deep42 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep42 { Deep43 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep43 { Deep44 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep44 { Deep45 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep45 { Deep46 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep46 { Deep47 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep47 { Deep48 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep48 { Deep49 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep49 { Deep50 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep50 { Deep51 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep51 { Deep52 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep52 { Deep53 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep53 { Deep54 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep54 { Deep55 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep55 { Deep56 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep56 { Deep57 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep57 { Deep58 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep58 { Deep59 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep59 { Deep60 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep60 { Deep61 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep61 { Deep62 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep62 { Deep63 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep63 { Deep64 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep64 { Deep65 value(); }
@Retention(RetentionPolicy.RUNTIME) @interface Deep65 { int value(); }

/** Never used: annotated 66 levels deep (#260 review). Its jar loads and runs. */
@Deep0(@Deep1(@Deep2(@Deep3(@Deep4(@Deep5(@Deep6(@Deep7(@Deep8(@Deep9(@Deep10(@Deep11(@Deep12(@Deep13(@Deep14(@Deep15(@Deep16(@Deep17(@Deep18(@Deep19(@Deep20(@Deep21(@Deep22(@Deep23(@Deep24(@Deep25(@Deep26(@Deep27(@Deep28(@Deep29(@Deep30(@Deep31(@Deep32(@Deep33(@Deep34(@Deep35(@Deep36(@Deep37(@Deep38(@Deep39(@Deep40(@Deep41(@Deep42(@Deep43(@Deep44(@Deep45(@Deep46(@Deep47(@Deep48(@Deep49(@Deep50(@Deep51(@Deep52(@Deep53(@Deep54(@Deep55(@Deep56(@Deep57(@Deep58(@Deep59(@Deep60(@Deep61(@Deep62(@Deep63(@Deep64(@Deep65(7))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))
final class DeepAnnotated {}
