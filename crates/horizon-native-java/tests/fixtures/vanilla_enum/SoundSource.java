package net.minecraft.sounds;
public enum SoundSource { MASTER("master"), PLAYERS("player"); private final String name; SoundSource(String n){name=n;} public String getName(){return name;} }
