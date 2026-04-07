# FlashBangROM GUI Doku

Diese Datei beschreibt den aktuellen Aufbau der Desktop-GUI in `FlashBangStudio` und dient als Entwickler-Guide fuer Aenderungen.

## 1) Grober Ueberblick

### Ziel der GUI
- Die GUI ist die interaktive Arbeitsflaeche fuer den ROM-Workflow.
- Kernbereiche sind:
  - Verbindung zum Geraet (Serial Connect, HELLO, ID)
  - Byte-Ansicht fuer Inspector (Chip-Snapshot) und Workbench (Bearbeitungsbereich)
  - Transfer-Operationen gemaess Operation-Matrix (Fetch, Flash, Erase, Copy, Load, Save)
  - Diff/Verify-Auswertung fuer Farb-/Statuslogik (ohne separaten Diff-Tab)
  - Serial-Monitor (TX/RX) zur Diagnose

### Laufzeitfluss (high level)
1. Start in `main.rs`:
   - Standardfall startet GUI (`run_gui`).
2. `run_gui()` in `gui.rs`:
   - Initialisiert das eframe/egui Fenster und erzeugt `FlashBangGuiApp`.
3. `FlashBangGuiApp::update()`:
   - Zeichnet Top-Bar (P), zentralen Arbeitsbereich, globalen Splitter und Serial-Monitor (S).
  - Verarbeitet Connect/Disconnect sowie getrennte Diagnoseaktionen (`ID`, `Upload Driver`, `Driver Abfragen`).
  - Rendert den Hex Workspace als zentrale Hauptansicht.
4. Aktionen im Hex-Workspace triggern:
   - Protokoll-Commands ueber Serial (z. B. `READ|...`, `PROGRAM_BYTE|...`)
  - Buffer-Updates (Inspector/Workbench)
   - Diff-Neuberechnung

### Datenmodell in der GUI
- `AppData` haelt:
  - Device/Firmware-Infos (`hello`, `chip`)
  - Inspector-Daten (`ro_data`, `ro_known`)
  - Workbench-Daten (`work_data`)
  - Diff-Ergebnis (`diff_report`)
  - GUI-Log (`log`)
- `FlashBangGuiApp` haelt zusaetzlich:
  - GUI-Zustand (Farbmodus, Cursor, Eingabefelder)
  - Serial-Verbindung (`serial_handle`, Ports, Wire-Log)
  - Icon-Assets und gecachte Composite-Textures

## 2) Detailaufbau nach Aspekt

## 2.1 Einstieg und App-Lebenszyklus
- Datei: `FlashBangStudio/src/main.rs`
  - Argumentrouting (`--gui`)
  - Startpunkt fuer GUI: `gui::run_gui()`
- Datei: `FlashBangStudio/src/gui.rs`
  - `run_gui()` erstellt eframe-Fenster
  - `impl eframe::App for FlashBangGuiApp` mit zentraler `update()`-Schleife

## 2.2 UI-Struktur (Panels)
- Datei: `FlashBangStudio/src/gui.rs`
  - Top Bar (P): Portwahl, Connect/Disconnect, `ID`, `Upload Driver`, `Driver Abfragen`, Statusanzeige inkl. kompakter Chip-Info
  - Center Panel: oberer Arbeitsbereich + globaler Splitter + Serial Monitor (S)
  - Serial Monitor nutzt die verbleibende Resthoehe dynamisch
- Renderer im oberen Arbeitsbereich:
  - `draw_hex_dump()`

## 2.2.1 Layout-Skizze Gesamtfenster

Die GUI folgt einem klassischen vertikalen Stack mit stabilem Kopf/Fuss und dynamischem Mittelpunkt:

```text
PPPPPPPPPPPPP

RRRRR G BBBBB
RRRRR G BBBBB
RRRRR G BBBBB
RRRRR G BBBBB
RRRRR G BBBBB

YYYYY   CCCCC

SSSSSSSSSSSSS
SSSSSSSSSSSSS
SSSSSSSSSSSSS
```

Legende:
- `P` = Navi/Infoleiste (inkl. Status)
- `R` = RO-Bereich / Inspector
- `G` = Transfer-Buttons (Copy/Flash)
- `B` = Workbench
- `Y` = Chip-Buttons (Fetch/Erase)
- `C` = File-Buttons
- `S` = Serial Monitor

Wichtig fuer Modifikationen:
- `R` und `B` muessen gleichzeitig sichtbar sein.
- `G` liegt immer zwischen `R` und `B`.
- Restbreite wird 50/50 auf `R` und `B` verteilt.
- `R + G + B` darf nie breiter als das Fenster sein.
- Status/Fehler laufen in `P` ueber `self.status`.
- Diagnosedaten gehoeren in den Serial-Monitor (`wire_log`) und nicht in die Statuszeile.
- Zwischen oberem Arbeitsbereich und Serial-Monitor existiert ein globaler vertikaler Splitter.
- Splitter-Default ist 75/25 (oben/serial), mit Mindesthoehen fuer beide Bereiche.

## 2.2.2 Layout-Skizze Hex Workspace (Haupt-Tab)

Der Hex-Workspace ist ein zweistufiges Raster (oben 3 Spalten, unten 2 Spalten):

```text
Obere Zeile (ca. 75% Hoehe)

+------------------------+ +----------------------+ +------------------------+
| R: Inspector           | | G: Transfer          | | B: Workbench           |
| - Byte Grid            | | - Copy Buttons       | | - Byte Grid            |
| - Cursor/Selection     | | - Flash Buttons      | | - Cursor/Selection     |
|                        | |                      | | - Direktbearbeitung    |
+------------------------+ +----------------------+ +------------------------+

Untere Zeile

+------------------------+                        +------------------------+
| Y: Chip Ops            |                        | C: File Ops            |
| - Fetch Image/Range/S  |                        | - Load Image/Sector    |
| - Erase Image/Sector   |                        | - Save Image/Sector    |
+------------------------+                        +------------------------+
```

Die Layout-Breite wird dynamisch aufgeteilt:
- Mittlere Transfer-Spalte ist kompakt auf Zielbreite `150.0` ausgelegt (120px Buttons plus Rand/Padding) und schrumpft nur bei sehr engen Fenstern.
- Linke und rechte Seite teilen den Rest gleichmaessig (`side_width`, 50/50).
- Hex-Zellen sind absichtlich kompakt gerendert, damit beide Panels sichtbar bleiben.
- Die Gesamtgeometrie bleibt responsive, ohne Ueberlappungen oder horizontales Ueberlaufen.

Panel-Struktur (analog zur HTML-Skizze):
- Linke Spalte: Inspector oben, Chip-Operationsbuttons direkt darunter, gleiche Spaltenbreite.
- Mitte: Transfer-Buttons (Copy/Flash) in eigener, schmaler Spalte.
- Rechte Spalte: Workbench oben, Disk-Operationsbuttons direkt darunter, gleiche Spaltenbreite.

## 2.2.3 Bedien-Skizze fuer Datenfluss

```text
Chip <-> Inspector <-> Workbench <-> Disk

Fetch:   Chip ----> Inspector
Copy:    Inspector -> Workbench
Flash:   Workbench -> Chip
Load:    Disk ----> Workbench
Save:    Workbench -> Disk
Erase:   Chip ----> Trash (logische Senke)
```

Hinweis:
- Diese Darstellung ist absichtlich semantisch (was passiert fachlich), nicht elektrisch/protokollarisch.
- Das passt zur Operation-Matrix aus dem MASTERPLAN.

## 2.2.4 Layout-Hotspots im Code

Wenn du nur das Layout anfassen willst, starte hier:
- `update()`:
  - Top-Bar, globale Splitter- und Resthoehenlogik.
- `draw_hex_dump()`:
  - komplette Geometrie fuer den Haupt-Tab, inklusive Spalten und Gruppen.
- `draw_byte_grid()`:
  - konkrete Darstellung der Bytezellen.

## 2.2.5 Mini-Guide: Layout sauber aendern

1. Erst Geometrie, dann Inhalte:
   - Breiten/Hoehen/Spaltenstruktur in `draw_hex_dump()` justieren.
2. Gruppen zuerst stabilisieren:
   - `ui.group(...)` Blöcke fuer R/G/B/Y/C anpassen, bevor Button-Logik geaendert wird.
3. Danach Scrollverhalten pruefen:
   - Byte-Grids und Button-Spalte bei kleiner Fensterhoehe testen.
4. Zum Schluss Lesbarkeit pruefen:
   - Statuszeile bleibt kurz.
   - Kritische Aktionen (Flash/Erase) bleiben visuell klar getrennt.

Beispiel: Mittlere Spalte breiter machen
- In `draw_hex_dump()` den Wert `transfer_col_width` anheben (z. B. von `150.0` auf `200.0`).
- Danach prüfen, ob linke/rechte Grid-Spalten noch genug Breite fuer 16 Bytes/Zeile haben.

Beispiel: Unten zusaetzlichen Operationsblock ergaenzen
- In der unteren `ui.horizontal_top(...)`-Sektion einen weiteren `allocate_ui_with_layout(...)`-Block einfuegen.
- Darin eine eigene `ui.group(...)`-Sektion mit klarer Ueberschrift nutzen.
- Falls horizontal zu eng: statt 3 Blocks auf 2 Blocks + umgebrochenes `horizontal_wrapped` wechseln.

## 2.3 Byte-Ansichten und Editierlogik
- Datei: `FlashBangStudio/src/gui.rs`
  - Grid-Rendering:
    - `draw_byte_grid()` fuer Inspector/Workbench
    - Byte-Farbgebung ueber `byte_color_for_ro()` / `byte_color_for_work()`
  - Tastatur/Copy/Paste:
    - `handle_workspace_typing()`
    - `decode_clipboard_hex()`, `paste_text_into_work()`, `copy_range_into_clipboard()`
  - Zeichenmodi:
    - Hex und ASCII (Latin-15) via `CharacterMode`, `decode_latin15()`, `encode_latin15()`

## 2.4 Transfer-/Operationslogik
- Datei: `FlashBangStudio/src/gui.rs`
  - Inspector <-> Workbench:
    - `copy_ro_into_work()`
  - Chip- und Transferaktionen laufen worker-basiert ueber:
    - `SerialWorkerRequest::{FetchRange, FlashRange, EraseChip, EraseSector}`
    - `SerialWorkerEvent::{FetchRangeCompleted, FlashRangeCompleted, EraseCompleted}`
  - Serial-Helfer fuer I/O:
    - `fetch_range_on_handle()`
    - `flash_range_on_handle()`
    - `erase_sector_on_handle()` / `erase_chip_on_handle()`
    - Schutzlogik ueber `ByteState`:
      - Gray: unbekannt/stale
      - Green: identisch
      - Orange: 1->0 programmierbar
      - Red: Erase noetig
  - Laufender Fortschritt:
    - Worker sendet `SerialWorkerEvent::Progress`
    - Anzeige als ProgressBar im Log-Bereich
  - Datei-IO:
    - `load_file_into_work()`, `save_work_range_to_file()`, `sector_file_path()`

## 2.5 Protokoll und Serial
- Datei: `FlashBangStudio/src/gui.rs`
  - Serial low-level:
    - `serial_send_and_read_lines_on_handle()`
    - `send_expect_ok_on_handle()`
  - Handshake/ID/Driver ueber Worker-Aktionen:
    - `QueryFirmware` (HELLO)
    - `QueryId` (ID-Sequenzen + ID-Abfrage)
    - `UploadDriver` (vollstaendiger Treiber-Upload)
    - `QueryDriver` (INSPECT)
  - Portverwaltung:
    - `refresh_ports()`
- Datei: `FlashBangStudio/src/protocol.rs`
  - `DeviceFrame` und `parse_device_frame()`
  - Parsing fuer `HELLO|`, `OK|`, `ERR|`, `DATA|`, `STATUS|`
- Datei: `FlashBangStudio/src/session.rs`
  - Serielle Infrastruktur (`list_serial_ports`, `open_serial_port`)
  - Datentypen (`HelloInfo`, `ChipId`)

## 2.6 Diff/Report
- Datei: `FlashBangStudio/src/verify.rs`
  - `compute_diff()` berechnet Byte-Mismatch-Liste
- Datei: `FlashBangStudio/src/report.rs`
  - `build_report()` + Gruppierung in Regionen
  - Export als TXT/JSON
- Datei: `FlashBangStudio/src/gui.rs`
  - `rebuild_diff_report()` wird nach Datenaenderungen aufgerufen
  - kein separater Diff-Tab; Diff bleibt intern fuer Vergleichslogik nutzbar

## 2.7 Icon-System (Operation Matrix)
- Datei: `FlashBangStudio/src/gui.rs`
  - Icon-Typen:
    - `BaseIcon`, `OverlayIcon`, `ArrowIcon`, `ButtonVisualSpec`
  - Asset-Laden:
    - `ensure_icon_assets_loaded()`
  - Komposition:
    - `texture_for_visual()` kombiniert 3x 40x40 Tiles zu 120x40
  - Rendering:
    - `operation_button()`
- Asset-Pfade:
  - `Resources/Assets/Buttons/base/`
  - `Resources/Assets/Buttons/overlays/`
  - `Resources/Assets/Buttons/arrows/`

## 2.8 Versionsanzeige
- Datei: `FlashBangStudio/build.rs`
  - schreibt Build/Git-Metadaten in Env-Variablen
- Datei: `FlashBangStudio/src/version.rs`
  - kapselt Zugriff auf diese Variablen
- Datei: `FlashBangStudio/src/gui.rs`
  - zeigt in der Top-Bar `version::package_version()`
  - zeigt im About-Dialog den Build-String via `version::version_text()`

## 3) Relevante Dateien fuer GUI-Aenderungen

### Primar
- `FlashBangStudio/src/gui.rs`
  - Fast alle UI-Komponenten, Operationsbuttons, Buffer-Logik, Serial-Wiring

### Sekundaer (haeufig mit-aendern)
- `FlashBangStudio/src/protocol.rs`
  - wenn neue Frame-Typen oder Parser-Regeln noetig sind
- `FlashBangStudio/src/session.rs`
  - wenn Port-Listing/Open-Verhalten geaendert wird
- `FlashBangStudio/src/verify.rs`
  - wenn Diff-Regeln angepasst werden
- `FlashBangStudio/src/report.rs`
  - wenn Diff-Exportstruktur angepasst wird
- `FlashBangStudio/src/version.rs` + `FlashBangStudio/build.rs`
  - wenn Versionsdarstellung/Build-Metadaten geaendert werden

### Assets
- `Resources/Assets/Buttons/**`
  - wenn Icons/Operation-Visuals geaendert werden

## 4) Vorgehensguide fuer Entwickler

## 4.1 Empfohlener Ablauf bei Modifikationen
1. Ziel klar abgrenzen:
   - UI-only, Protokoll-only, oder End-to-End?
2. Betroffene Code-Pfade markieren:
   - UI-Renderebene (`draw_*`) vs. Aktionslogik (`*_range_*`, `erase_*`, `flash_*`)
3. Aenderung zuerst klein in `gui.rs` umsetzen.
4. Falls noetig Protokoll/Session erweitern.
5. Danach Testen:
   - Unit-Tests (`cargo test`)
   - Manuell GUI-Flow mit Hardware oder Mock pruefen
6. Doku aktualisieren (mindestens diese Datei und DEV_LOG, falls Ausfuehrungsschritt relevant).

## 4.2 Beispiel A: Neue Operationstaste hinzufuegen
Beispiel: Neue Range-basierte Aktion in der Transfer-Spalte.

1. Aktion als Methode in `FlashBangGuiApp` anlegen, z. B. `fn verify_range_live(...)`.
2. In `draw_hex_dump()` im passenden UI-Block eine weitere `operation_button(...)`-Instanz einfuegen.
3. Das passende `ButtonVisualSpec` waehlen:
   - `left_base`, `left_overlay`, `arrow`, `right_overlay`, `right_base`
4. Im Click-Handler:
   - Eingaben validieren (`parse_range_input()`)
   - Aktion ausfuehren
   - `self.status` setzen
5. Wenn Daten geaendert wurden:
   - `self.rebuild_diff_report()` aufrufen.

## 4.3 Beispiel B: Farbregeln fuer Bytes anpassen
Beispiel: Andere Semantik in Diff-Ansicht.

1. `ByteState` und `byte_state()` in `gui.rs` anpassen.
2. Mapping in `diff_color_for_state()` aktualisieren.
3. Sicherstellen, dass die Workbench-Farbe (`byte_color_for_work`) konsistent bleibt.
4. Vorhandene Tests zu `ByteState` im `#[cfg(test)]`-Block von `gui.rs` aktualisieren/erweitern.

## 4.4 Beispiel C: Neues Protokollframe in GUI nutzen
Beispiel: Firmware sendet neuen `STATUS|...`-Detailinhalt.

1. Parser in `protocol.rs` pruefen/erweitern (`DeviceFrame`, `parse_device_frame`).
2. GUI-Pfad anpassen, der die Antwort konsumiert (z. B. `send_expect_ok`, Worker-Event-Flow, FW-Abfrage).
3. Status-/Log-Ausgabe in GUI ergaenzen.
4. Parser-Tests in `protocol.rs` erweitern.

## 4.5 Aktuelle Buttons Und GUI-Funktionen (Stand jetzt)

Die folgende Liste beschreibt alle aktuell sichtbaren Bedienfunktionen mit genau einer Kurzzeile, wann der User sie braucht.

### 5.1 Top-Bar (Verbindung/Diagnose)
- `⟳` bei Serial Port: Portliste neu einlesen, wenn Geraet neu angesteckt wurde oder Port gewechselt hat.
- `Connect`: Startet den kompletten Verbindungsablauf (`open -> HELLO -> ID-Check -> Driver-Upload -> optional Auto-Fetch`).
- `Disconnect`: Trennt die aktive serielle Verbindung sauber.
- `ID`: Fuehrt nur ID-Sequenz-Set + ID-Abfrage aus, um den Chip schnell zu identifizieren.
- `Upload Driver`: Laedt alle Parameter/Sequenzen aus dem aktuell gewaehlten Treiber auf das Zielgeraet.
- `Driver Abfragen`: Fuehrt `INSPECT` aus, um die aktuell am Geraet gesetzten Driver-Daten zu sehen.
- `⟳` bei Treiberauswahl: Laedt die Treiber-YAMLs neu, wenn Dateien in `drivers/` geaendert wurden.
- `About`: Zeigt Versions-/Build-/Git-Informationen der Anwendung.

### 5.2 Arbeitsbereich (Filter/Optionen)
- `Diff` (Toggle): Schaltet Diff-basierte Vordergrundfarben fuer Bytezellen ein/aus.
- `Palette` (Toggle): Schaltet RGB332-Hintergrundfarben der Bytezellen ein/aus.
- `Show Sector Boundaries` (Toggle): Zeigt Sektorgrenzen und Sektorlabels zur Orientierung im Grid.
- `Allow Flash on gray` (Toggle): Erlaubt Flash trotz unbekannter Inspector-Zellen (grau).
- `Auto-Fetch` (Toggle): Bestimmt, ob nach passenden Aktionen automatisch aus dem Chip nachgelesen wird.
- `Preview Window` (Toggle): Oeffnet die Workbench-Bildvorschau fuer visuelle Byte-Musterkontrolle.
- `PNG Import` (Toggle): Oeffnet den PNG-Importdialog fuer quantisierte Workbench-Slices.
- `New Workbench`: Erstellt eine neue leere Workbench in gewaehlter Groesse.

### 5.3 Chip-Operationsblock (links unten)
- `Fetch Image`: Liest den kompletten Chip in den Inspector.
- `Fetch Range`: Liest nur den angegebenen Bereich in den Inspector.
- `Fetch Sector`: Liest nur den gewaehlten Sektor in den Inspector.
- `Erase Image`: Loescht den kompletten Chip.
- `Erase Sector`: Loescht nur den gewaehlten Sektor.

### 5.4 Transfer-Spalte (Mitte)
- `Copy Image`: Kopiert kompletten bekannten Inspector-Inhalt in die Workbench.
- `Copy Range`: Kopiert den gewaehlten bekannten Bereich in die Workbench.
- `Copy Sector`: Kopiert den gewaehlten bekannten Sektor in die Workbench.
- `Flash Image`: Programmiert den kompletten Workbench-Inhalt in den Chip (nur wenn sinnvoll/freigegeben).
- `Flash Range`: Programmiert nur den gewaehlten Bereich in den Chip.
- `Flash Sector`: Programmiert nur den gewaehlten Sektor in den Chip.

### 5.5 Datei-Operationsblock (rechts unten)
- `Load Image`: Laedt ein komplettes Image von Disk in die Workbench.
- `Load Sector`: Laedt Sektordaten von Disk in den aktuell gewaehlten Sektor der Workbench.
- `Save Image`: Speichert die komplette Workbench auf Disk.
- `Save Sector`: Speichert nur den aktuell gewaehlten Sektor der Workbench.

### 5.6 Serial-Monitor
- `Clear`: Leert die Log-Ansicht fuer eine neue, saubere Diagnosesession.
- Farbcodierung RX/TX/UI: Beschleunigt Diagnose (`TX` rot, `RX` gruen, `RX #...` grau-gruen, `RX OK...` lime, `UI` blau).
- `Fetch/Flash Progress`: Laufender Fortschritt wird als ProgressBar (Prozent + `current/total`) im Log-Bereich angezeigt.

### 5.7 Permanente Status-Chips
- `IMAGE`: Zeigt den Gesamtzustand Inspector vs. Workbench fuer das ganze Image (grau/orange/rot/gruen) inkl. Tooltip-Details.
- `SECTOR`: Zeigt den Gesamtzustand fuer den aktuell gewaehlten Sektor (grau/orange/rot/gruen) inkl. Tooltip-Details.

## 4.6 Beispiel D: Layout-Bereich erweitern
Beispiel: Neuer Statistik-Block in der Hauptansicht.

1. In `FlashBangGuiApp` ggf. neuen Zustand hinzufuegen.
2. Den Bereich in `draw_hex_dump()` an geeigneter Stelle einfuegen.
3. Neue Renderfunktion erstellen, z. B. `draw_stats_panel()`.
4. Keine schwere Logik direkt im Rendercode, sondern in Hilfsmethoden auslagern.

## 5) Coding-Hinweise (spezifisch fuer diese GUI)

- Achte auf Trennung von:
  - Rendercode (`draw_*`)
  - Aktionslogik (Datenmutation/IO)
  - Parser/Protokoll (`protocol.rs`)
- Nach jeder Datenmutation in Inspector/Workbench Diff aktualisieren.
- `ro_known` immer konsistent halten:
  - Nach Flash/Erase betroffene Inspector-Bereiche als unknown markieren.
- Fehlermeldungen immer in `self.status` sichtbar machen.
- Bei neuen Icons:
  - zwingend 40x40 px pro Tile (wird validiert).

## 6) Schnellstart fuer neue Entwickler

1. In `FlashBangStudio` wechseln.
2. Tests laufen lassen:
   - `cargo test`
3. GUI starten:
   - `cargo run -- --gui`
4. Fuer GUI-Arbeit zuerst `gui.rs` lesen, dann je nach Thema `protocol.rs`, `session.rs`, `verify.rs`, `report.rs`.

## 7) TODO fuer kuenftige GUI-Doku-Vertiefung

- Exakte Mapping-Tabelle: Operation-Matrix (MASTERPLAN) <-> konkrete `operation_button`-Keys in `gui.rs`.
- Sequenzdiagramm fuer Connect -> HELLO -> ID -> Buffer-Allokation.
- Kurze Troubleshooting-Sektion fuer Serial-Timeouts und stale-gray Verhalten.
