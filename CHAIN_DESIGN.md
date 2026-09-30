# Pedal chain (signal chain) — how it's written to the amp

## Storage
The chain is **20 bytes at section 0x06, indices 0x00–0x13** (the "panel" section,
frame `60 00 06 00` in the patch). Byte value = effect ID; the order of the bytes
IS the signal chain order. Writing = reordering those 20 bytes and sending
section 0x06 (the C++ calls `setFileSource("Structure", "06", "00", "00", hexData)`
→ `dataChange` → DT1 frame to the temp buffer).

## Effect ID map (byte value → pedal name)
| ID | Name | ID | Name |
|----|------|----|------|
| 00 | CS (Compressor) | 0A | EQ2 |
| 01 | LP (Limiter) | 0B | PDL (Pedal) |
| 02 | CH_A | 0C | FV (Foot Volume) |
| 03 | CH_B | 0D | NS_1 (Noise Suppressor 1) |
| 04 | EQ1 | 0E | NS_2 |
| 05 | FX1 | 0F | OD (Overdrive) |
| 06 | FX2 | 10 | USB |
| 07 | DD1 (Delay 1) | 11 | CN_S (Speaker Sim?) |
| 08 | DD2 (Delay 2) | 12 | CAB |
| 09 | RV (Reverb) | 13 | CN_M |

## Default chain (from default.syx)
`11 CN_S, 0B PDL, 0F OD, 05 FX1, 06 FX2, 0D NS_1, 04 EQ1, 02 CH_A, 0A EQ2, 0C FV,
01 LP, 07 DD1, 08 DD2, 09 RV, 12 CAB, 00 CS, 03 CH_B, 13 CN_M, 0E NS_2, 10 USB`

## Chain position policy bytes (0x19–0x23, also in section 0x06)
- 0x19: EQ2 position (0 = PreAmp In, 1 = Pre Amp Out)
- 0x20: S/R loop position (0x07 or 0x11 depending on val at 0x20=3/6)
- 0x21: S/R loop post amp (0/1)
- 0x22: EQ1 position (0/1)
- 0x23: PDL position (0/1)
The C++ `chain_Set` functions reorder the 20 bytes AND set these policy bytes
when the user moves a chain item.

## Implementation plan (graphical chain editor)
1. Read section 0x06 bytes 0x00–0x13 → ordered array of effect IDs.
2. Render as a horizontal chain of pedal tiles (use Boss pedal image map when
   coordinates are available; else styled tiles with the effect name).
3. Edit: drag to reorder / click-to-insert from a palette.
4. Write: send section 0x06 with the new 20-byte order + policy bytes.

## Boss Tone Studio reference
BTS has a chain editor: fixed positions (some effects can't move), drag between.
The Katana MK2 hardware imposes ordering constraints (e.g. OD before amp, DD/RV
at fixed spots) — enforce those client-side while allowing free recombination
within the movable set (per C++ chain_*_Set functions: the four trailer IDs
03/0E/10/13 (CH_B/NS_2/USB/CN_M) are appended at the end, and 01 (S/R) has
policy, EQ1/EQ2/PDL have position toggles).
