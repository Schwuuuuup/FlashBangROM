# DriverBasedCommunication — Design-Dokument

> Branch: `DriverBasedCommunication`
> Status: Entwurf — offene Entscheidungen werden hier festgehalten

## Ziel

Die hardcodierten SST39-Chip-Kommandosequenzen in der Firmware durch ein generisches
Micro-Script-System ersetzen. Chip-Treiber werden als YAML-Dateien gepflegt und können
per Protokoll zur Laufzeit in die Firmware geladen werden. Die SST39-Sequenzen bleiben
als Default im Firmware-RAM.

## Anforderungen

### Primitiven (aus Code-Analyse)

Alle Chip-Operationen lassen sich auf 5 Primitiven zurückführen:

| Primitiv | Opcode | Beschreibung | Firmware-Referenz |
|----------|--------|-------------|-------------------|
| WRITE(addr, data) | `W` | Bus-Write-Cycle | `writeCycle()` in `hal_bus.cpp` |
| READ(addr) → Wert | `R` | Bus-Read-Cycle | `readCycle()` in `hal_bus.cpp` |
| DELAY(µs) | `D` | Timing-Pause | `delayMicroseconds()` |
| TOGGLE(addr, timeout) | `T` | DQ6-Toggle bis stabil | `waitToggleDone()` in `sst39_ops.cpp` |
| POLL_DQ7(addr, expected, timeout) | `P` | Bit 7 vs. Expected | `waitDq7DoneProgram()` in `sst39_ops.cpp` |

### Variablen (vom Kontext gesetzt)

| Variable | Bedeutung | Gesetzt durch |
|----------|-----------|---------------|
| `$A` | Ziel-Adresse (bis 19 bit) | Firmware vor Sequenz-Ausführung |
| `$D` | Ziel-Datenbyte (8 bit) | Firmware vor Program-Sequenz |
| `$R0`, `$R1` | Gelesene Werte | Sequenz-Ausführung (READ-Ergebnis) |
| `$0`..`$7` | Custom-Parameter (32 bit) | Per `PARAMETER|<key>|<value>` (lowercase), in Sequenzen als `$0`..`$7` referenziert |

*`$SA` entfällt — der Chip maskiert bei Sector-Erase die unteren Adressbits selbst (A11..A0 werden ignoriert). `$A` reicht.*

### Parameter-Konvention (Groß/Klein)

- **GROSSBUCHSTABEN** → Built-in Parameter, intern von der Firmware genutzt:
  `PARAMETER|CHIP_SIZE|80000`, `PARAMETER|SECTOR_SIZE|1000`, `PARAMETER|ADDR_BITS|13`
- **kleinbuchstaben** → Custom Parameter, Key-Value Store für Sequenz-Variablen `$0`..`$7`:
  `PARAMETER|page_size|80` → wird als `$0` (erster Custom-Param) in Sequenzen verfügbar

Die Zuordnung Key→Slot-Nummer erfolgt in Reihenfolge der Definition. `INSPECT` gibt beides aus.

### Benötigte Sequenzen pro Treiber

1. **id_entry** — Chip in ID-Modus versetzen
2. **id_read** — Manufacturer + Device ID lesen → `$R0`, `$R1`
3. **id_exit** — Chip aus ID-Modus zurücksetzen
4. **program_byte** — Einzelnes Byte programmieren (inkl. Polling)
5. **sector_erase** — Sektor löschen (inkl. Polling)
6. **chip_erase** — Gesamten Chip löschen (inkl. Polling)

*Read benötigt keine Sequenz — ist ein generischer Bus-Read-Cycle.*

---

## Design-Entscheidungen

### DE-1: Micro-Script Syntax

**Frage:** Wie soll die Skript-Syntax für Sequenzen aussehen?

| Option | Format | Pro | Contra |
|--------|--------|-----|--------|
| **A** | Kompakt, `;`-getrennt: `W5555,AA;W2AAA,55` | Minimal RAM, ein String = eine Sequenz | Weniger lesbar |
| **B** | Zeilenbasiert: `WRITE 0x5555 0xAA` | Sehr gut lesbar im Serial-Monitor | Multi-Line-Transfer komplexer |
| **C** | JSON inline | Strukturiert | Syntax-Overhead, RAM-hungrig |

**Entscheidung:** ✅ **Option A — Kompakt, semikolon-getrennt**
Beispiel: `W5555,AA;W2AAA,55;W5555,A0;W$A,$D;T$A,50000`

---

### DE-2: Zahlenformat in Micro-Script

**Frage:** Welches Zahlenformat für Adressen, Daten und Timeouts?

| Option | Beispiel | Pro | Contra |
|--------|----------|-----|--------|
| **A** | Alles Hex ohne Prefix: `W5555,AA`, Timeout `C350` | Einheitlich, kompakt | Timeouts in Hex nicht intuitiv |
| **B** | Hex für Addr/Data, Dezimal für Timeouts | Timeouts lesbar | Kontexterkennung nötig |
| **C** | Mit `0x`-Prefix: `W0x5555,0xAA` | Eindeutig | Mehr Bytes pro Befehl |

**Entscheidung:** ✅ **Option B — Hex für Addr/Data, Dezimal für Timeouts**
Adressen/Daten als Hex ohne Prefix (`5555`, `AA`), Timeouts als Dezimal in µs (`50000`).
Befehlserkennung: Timeout-Parameter folgt auf Befehle `T`/`P` → Dezimal. Alles andere → Hex.

---

### DE-3: Polling-Methode — in Sequenz oder global?

**Frage:** Soll das Completion-Polling Teil der Sequenz sein oder ein globaler Treiber-Parameter?

| Option | Beschreibung | Pro | Contra |
|--------|-------------|-----|--------|
| **A** | Polling als Befehl in Sequenz: `T$A,C350` | Flexibel pro Operation, verschiedene Timeouts möglich | Sequenz etwas länger |
| **B** | Globale Treiber-Einstellung, Firmware pollt automatisch | Kürzere Sequenzen | Nicht jede Op braucht Polling, weniger flexibel |

**Entscheidung:** ✅ **Option A — Polling als Befehl in der Sequenz**
Jede Sequenz definiert selbst, ob und wie gepollt wird.
`id_entry` hat kein Polling, `program_byte` hat `T$A,50000`, `sector_erase` hat `T$A,50000000`.

---

### DE-4: Protokoll-Upload-Granularität

**Frage:** Wie werden Treiber-Daten über das Protokoll übertragen?

| Option | Beschreibung | Pro | Contra |
|--------|-------------|-----|--------|
| **A** | Einzelbefehle, jeder einzeln bestätigt | Einfach, debuggbar, human-readable | Mehr Round-Trips |
| **B** | Block-Upload: `DRIVER_UPLOAD|BEGIN` ... `DRIVER_UPLOAD|END` | Atomar | Komplexerer Parser |

**Entscheidung:** ✅ **Option A — Einzelbefehle, jeder bestätigt**
Jeder `SEQUENCE`/`PARAMETER`-Befehl wird einzeln gesendet und mit `OK`/`ERR` bestätigt.
Manuell per minicom testbar.

---

### DE-5: Maximale Sequenz-Länge (Firmware)

**Frage:** Wie viel RAM pro Sequenz-String?

| Option | Beschreibung | Pro | Contra |
|--------|-------------|-----|--------|
| **A** | Fix 64–96 Bytes pro Sequenz (~700 Bytes gesamt) | Vorhersagbar, reicht für alle bekannten Chips | Theoretisch limitiert |
| **B** | Dynamische Allokation | Maximale Flexibilität | Fragmentierungsrisiko auf STM32 |

**Entscheidung:** ✅ **Option A — Fix 96 Bytes pro Sequenz**
12 Slots × (20 Name + 96 Script) + Parameter ≈ 1.6 KB RAM gesamt. Akzeptabel bei 20KB STM32F103.

---

### DE-6: Feste vs. dynamische Sequenz-Slots

**Frage:** Sollen Sequenz-Slots fest benannt (6 Felder) oder dynamisch (Name-String + Script) sein?

| Option | Beschreibung | Pro | Contra |
|--------|-------------|-----|--------|
| **A** | Dynamische Key-Value Slots (max 12) | Beliebige Sequenzen möglich (write_protect etc.) | Lookup per strcmp(), etwas mehr RAM |
| **B** | Feste Slots + Extra-Slots | Einfacherer Zugriff auf well-known | Weniger flexibel |
| **C** | Rein feste Slots | Einfachster Code | Widerspricht Treiber-Konzept |

**Entscheidung:** ✅ **Option A — Dynamische Key-Value Slots**
Max 12 Sequenzen pro Treiber. Well-known Names (`id_entry`, `program_byte` etc.) werden per `strcmp()` gesucht. Beliebige Extra-Sequenzen (z.B. `write_protect`) direkt als Kleinbuchstaben-Befehl aufrufbar.

---

### DE-7: Bulk-Write / Page-Write

**Frage:** Wie soll Bulk-Write behandelt werden?

| Option | Beschreibung | Pro | Contra |
|--------|-------------|-----|--------|
| **A** | Loop in Firmware, Script pro Byte | Einfach, SST39-first | Kein Page-Write möglich |
| **B** | Loop-Konstrukt in Micro-Script | Page-Write-Chips unterstützt | Parser komplexer |
| **C** | Nicht jetzt lösen | SST39-first | Späteres Refactoring |

**Entscheidung:** ✅ **Option B — Loop-Konstrukt in Micro-Script**

---

### DE-8: Loop-Syntax

**Frage:** Wie wird der Loop-Body markiert?

| Option | Beschreibung | Pro | Contra |
|--------|-------------|-----|--------|
| **A** | `{...}` Geschweifte Klammern | Kompakt, Setup/Loop/Teardown in einem String | Eine Ebene der Verschachtelung |
| **B** | Drei separate Sequenzen | Simpler pro Script | Mehr Slots nötig |
| **C** | Firmware-Loop, kein Script-Wissen | Einfach | Kein Page-Write |

**Entscheidung:** ✅ **Option A — Geschweifte Klammern `{}`**
- Vor `{` = Setup (einmal)
- Innerhalb `{...}` = Body (pro Byte, `$A`/`$D` auto-increment)
- Nach `}` = Teardown (einmal)

---

### DE-9: Custom-Sequenz-Aufruf

**Frage:** Wie werden custom Sequenzen aufgerufen?

| Option | Beschreibung |
|--------|-------------|
| **A** | `EXEC|<name>` als generischer Befehl |
| **B** | Kleingeschriebene Befehle direkt — Namensraum-Trennung per Groß/Klein |

**Entscheidung:** ✅ **Option B — Namensraum-Konvention**
- GROSSBUCHSTABEN → Built-in (Bit 5 = 0)
- kleinbuchstaben → Custom-Sequenz (Bit 5 = 1)
- Erkennung: `if (line[0] & 0x20)` — ein einziges Bit
- `EXEC`-Befehl entfällt

---

### DE-10: Sequenz-Name für Program

**Frage:** Eine oder zwei Program-Sequenzen?

| Option | Beschreibung |
|--------|-------------|
| **A** | Einheitlich `program` für Single + Bulk |
| **B** | Getrennt: `program_byte` und `program_range` |

**Entscheidung:** ✅ **Option B — Getrennte Sequenzen**
- `program_byte` = Einzelbyte, kein Loop
- `program_range` = Bulk mit `{}`-Loop
- Wenn `program_range` nicht definiert → Fallback auf `program_byte` × N

---

### DE-11: Custom Parameter

**Frage:** Sollen Custom Parameter (für Sequenz-Variablen) jetzt oder später implementiert werden?

| Option | Beschreibung | Pro | Contra |
|--------|-------------|-----|--------|
| **A** | Jetzt, Referenz als `$name` | Lesbar | Variable-length Parser |
| **B** | Jetzt, Referenz als `$0`.`$7` | Einfacher Parser (fixed-width) | Weniger lesbar |
| **C** | Nur Built-in jetzt, Custom später | Simplerer Interpreter | Weniger flexibel |

**Entscheidung:** ✅ **Option B — Custom Parameter als `$0`..`$7`**
- GROSSBUCHSTABEN-Parameter (`PARAMETER|CHIP_SIZE|80000`) → Built-in, Firmware-intern
- kleinbuchstaben-Parameter (`PARAMETER|page_size|80`) → Custom, Key-Value Store
- In Sequenzen als `$0`..`$7` referenziert (Slot-Nummer = Reihenfolge der Definition)
- `INSPECT` gibt beides aus, Custom mit Name + Slot-Nummer
- RAM: 8 × 20 = 160 Bytes — vernachlässigbar
- `$SA` entfällt — Chip maskiert Sektoradresse selbst, `$A` reicht

### Zukunft: StudioSequences

*Nicht Teil der aktuellen Implementierung.* Spätere Erweiterung:
FlashBangStudio kann Sequenz-Templates mit aufwändiger String-Ersetzung verarbeiten
und nur das Endresultat an die Firmware senden. Die Firmware bemerkt davon nichts.

---

## Treiber-Dateiformat

Erweiterung des bestehenden `drivers/schema/chip-driver.schema.json`:

```yaml
# Beispiel: drivers/chips/sst39-core.yaml
id: sst39-core
family: SST39
name: "SST39SF Flash Family"
sector_size_bytes: 4096
address_bits: 19

models:
  - jedec_id: "0xBFB5"
    name: "SST39SF010A"
    size_bytes: 131072
  - jedec_id: "0xBFB6"
    name: "SST39SF020A"
    size_bytes: 262144
  - jedec_id: "0xBFB7"
    name: "SST39SF040"
    size_bytes: 524288

sequences:
  id_entry: "W5555,AA;W2AAA,55;W5555,90;D10"
  id_read: "R0000>R0;R0001>R1"
  id_exit: "W5555,AA;W2AAA,55;W5555,F0;D10"
  program_byte: "W5555,AA;W2AAA,55;W5555,A0;W$A,$D;PT$A,50000"
  program_range: "{W5555,AA;W2AAA,55;W5555,A0;W$A,$D;PT$A,50000}"
  sector_erase: "W5555,AA;W2AAA,55;W5555,80;W5555,AA;W2AAA,55;W$A,30;PT$A,50000000"
  chip_erase: "W5555,AA;W2AAA,55;W5555,80;W5555,AA;W2AAA,55;W5555,10;PT0000,250000000"
```

## Protokoll-Erweiterung

### Neue Befehle

| Befehl | Richtung | Beschreibung |
|--------|----------|-------------|
| `SEQUENCE\|<name>\|<script>` | Host→FW | Sequenz setzen |
| `PARAMETER\|<key>\|<value>` | Host→FW | Konfigurations-Parameter setzen |
| `INSPECT` | Host→FW | Alle Parameter + Sequenzen in setzbarer Syntax ausgeben |
| `DRIVER_RESET` | Host→FW | Zurück zu SST39-Defaults |
| `<lowercase_name>` | Host→FW | Custom-Sequenz direkt als Befehl |

### Namensraum-Konvention

Alle Befehle MÜSSEN mit einem Buchstaben beginnen.

- **GROSSBUCHSTABE** (ASCII Bit 5 = 0) → Built-in Befehl
- **kleinbuchstabe** (ASCII Bit 5 = 1) → Custom-Sequenz aus dynamischem Slot

Erkennung: `if (line[0] & 0x20)` → Custom, sonst Built-in.
Custom-Namen können Built-ins niemals überschreiben.

Gleiche Konvention gilt für `PARAMETER`:
- **GROSSBUCHSTABEN** → Built-in (`PARAMETER|CHIP_SIZE|80000`) — von Firmware intern genutzt
- **kleinbuchstaben** → Custom (`PARAMETER|page_size|80`) — Key-Value Store, in Sequenzen als `$0`..`$7` referenziert

Variablen in Sequenzen: `$A` (Adresse), `$D` (Datenbyte), `$R0`/`$R1` (Read-Ergebnis), `$0`..`$7` (Custom-Params)

Beispiel:
```
write_protect
OK|write_protect
```

### Well-known Sequenz-Namen

| Name | Aufgerufen bei | Loop | Beschreibung |
|------|----------------|------|-------------|
| `id_entry` | `ID` | nein | Chip in ID-Modus |
| `id_read` | `ID` | nein | IDs lesen → $R0, $R1 |
| `id_exit` | `ID` | nein | ID-Modus verlassen |
| `program_byte` | `PROGRAM_BYTE` | nein | Einzelbyte programmieren |
| `program_range` | `PROGRAM_RANGE` | ja `{}` | Bulk-Write (Fallback: `program_byte` × N) |
| `sector_erase` | `SECTOR_ERASE` | nein | Sektor löschen |
| `chip_erase` | `CHIP_ERASE` | nein | Chip löschen |

Alle weiteren Sequenzen sind frei benennbar und werden direkt als Kleinbuchstaben-Befehl gesendet.

### Loop-Konstrukt `{...}`

Geschweifte Klammern markieren den per-Byte Loop-Body in Sequenzen:
- Vor `{` = Setup (einmal)
- Innerhalb `{...}` = Body (pro Byte, `$A`/`$D` auto-increment)
- Nach `}` = Teardown (einmal)

```
SST39 (kein Page-Write):  {W5555,AA;W2AAA,55;W5555,A0;W$A,$D;PT$A,50000}
Page-Write-Chip:          W5555,AA;W2AAA,55;W5555,A0;{W$A,$D};PT$A,50000
```

### Workflow

```
Connect → HELLO
  → [optional: SEQUENCE × 6, PARAMETER × N]
  → ID
  → Host identifiziert Chip anhand $R0/$R1 in lokaler Treiber-YAML
  → Host sendet PARAMETER|CHIP_SIZE|<size>
  → Operationen (READ, PROGRAM_BYTE, SECTOR_ERASE, ...)
```

### HELLO Erweiterung

```
HELLO|flashbang-fw-0.4.0|0.2|sst39-core,data-hex,driver-upload
```

Neue Capability: `driver-upload`

## Implementierungsreihenfolge

1. Design-Entscheidungen treffen (DE-1 bis DE-11)
2. Treiber-Schema + `sst39-core.yaml` erweitern
3. Firmware: `DriverSlot` struct + SST39-Defaults
4. Firmware: Sequenz-Interpreter (`seq_interpreter.cpp/h`)
5. Firmware: `chip_probe.cpp` + `sst39_ops.cpp` → Interpreter nutzen
6. Firmware: Neue Protokoll-Befehle
7. Test: `protocol_smoke.py` erweitern
8. Hardware-Validierung
9. FlashBangStudio: Driver-Loading + GUI *(erst nach 8)*

## Betroffene Dateien

### Firmware
- `include/device_types.h` — DriverSlot struct, neue CommandType-Enums
- `include/device_globals.h` + `src/device_globals.cpp` — g_driverSlot
- `src/command_parser.cpp` — Neue Befehle parsen
- `src/command_executor.cpp` — Neue Befehle ausführen
- `src/sst39_ops.cpp` — Auf Interpreter umstellen
- `src/chip_probe.cpp` — Interpreter statt hardcoded nutzen
- **NEU:** `src/seq_interpreter.cpp` + `include/seq_interpreter.h`

### Studio (Rust)
- `src/session.rs` — `upload_driver()` Methode
- `src/protocol.rs` — Neue Befehle
- **NEU:** `src/driver_parser.rs` + `src/driver_upload.rs`

### Schema/Treiber
- `drivers/schema/chip-driver.schema.json` — Erweitern
- `drivers/chips/sst39-core.yaml` — Sequenzen hinzufügen
