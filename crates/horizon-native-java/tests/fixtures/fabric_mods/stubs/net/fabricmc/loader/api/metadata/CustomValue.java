package net.fabricmc.loader.api.metadata;
public interface CustomValue {
    CvType getType();
    CvObject getAsObject();
    CvArray getAsArray();
    String getAsString();
    Number getAsNumber();
    boolean getAsBoolean();
    interface CvObject {
        int size();
        boolean containsKey(String key);
        CustomValue get(String key);
        java.util.Iterator<java.util.Map.Entry<String,CustomValue>> iterator();
    }
    interface CvArray {
        int size();
        CustomValue get(int index);
        java.util.Iterator<CustomValue> iterator();
    }
    enum CvType { OBJECT, ARRAY, STRING, NUMBER, BOOLEAN, NULL }
}
