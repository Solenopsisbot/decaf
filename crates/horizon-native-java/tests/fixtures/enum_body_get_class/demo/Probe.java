package demo;
public class Probe {
  enum Body { A { }, B { }, C }
  public static String rows() {
    Object anon = new Object(){};
    return Body.A.getClass().getName()+"|"+Body.B.getClass().getName()+"|"
      +(Body.A.getClass()==Body.class)+"|"+((Object)Body.A instanceof String)+"|"+String.valueOf(Body.A.getClass().getDeclaringClass())+"|"
      +Body.A.getClass().getSuperclass().getName()+"|"+anon.getClass().getName();
  }
  public static void main(String[] a){System.out.print(rows());}
}
