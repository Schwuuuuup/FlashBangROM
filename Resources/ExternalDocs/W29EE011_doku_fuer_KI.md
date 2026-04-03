# W29EE011 – 128K × 8 CMOS Flash Memory

**Veröffentlichung:** August 1998  
**Kapazität:** 1 Mbit (128K × 8)

---

## 1. Allgemeine Beschreibung

Der W29EE011 ist ein CMOS-Flash-Speicher mit 1 Mbit Kapazität, organisiert als 128K × 8 Bit.

- Betrieb ausschließlich mit 5 V
- Programmierung und Löschung im System möglich
- Kein 12 V VPP erforderlich
- Schnelle Schreib-/Löschzyklen
- Niedriger Stromverbrauch

---

## 2. Hauptmerkmale

### Betrieb und Performance

- 5V-only Programmierung und Löschung
- Page-Write:
  - 128 Bytes pro Seite
  - max. 10 ms pro Seite
  - ~39 µs pro Byte (effektiv)
- Chip-Erase: 50 ms

### Zugriff

- Zugriffszeiten: 70 / 90 / 120 / 150 ns

### Lebensdauer

- 100 bis 1000 Programm-/Erase-Zyklen
- Datenerhalt: 10 Jahre

### Stromverbrauch

- Aktiv: ~25 mA
- Standby: ~20 µA

### Schutzmechanismen

- Software-Datenschutz
- Hardware-Datenschutz
- Noise-/Glitch-Schutz
- Write-Inhibit-Logik

---

## 3. Pinbeschreibung

### Signale

| Pin        | Beschreibung |
|------------|--------------|
| A0–A16     | Adressleitungen |
| DQ0–DQ7    | Datenleitungen (I/O) |
| CE         | Chip Enable |
| OE         | Output Enable |
| WE         | Write Enable |
| VDD        | Versorgung |
| GND        | Masse |
| NC         | Nicht verbunden |

---

## 4. Interne Struktur (Blockdiagramm – Beschreibung)

Der Speicher besteht aus:

- Adressdecoder (A0–A16)
- Speicherzellen-Array
- Steuerlogik (CE, OE, WE)
- Ausgangspuffer für Datenleitungen (DQ0–DQ7)

---

## 5. Funktionsbeschreibung

### 5.1 Read Mode

- CE = LOW
- OE = LOW
→ Daten werden ausgegeben

---

### 5.2 Page Write Mode

- 128 Bytes pro Seite
- Nicht geschriebene Bytes → FFh

---

### 5.3 Software-Datenschutz

Aktivierung:

5555H → AAH  
2AAAH → 55H  
5555H → A0H  

Deaktivierung:

5555H → AAH  
2AAAH → 55H  
5555H → 80H  
5555H → AAH  
2AAAH → 55H  
5555H → 20H  

---

### 5.4 Hardware-Datenschutz

- WE < 15 ns → ignoriert
- Schreiben gesperrt bei VDD < 3.8 V

---

### 5.5 Status-Erkennung

- DQ7: Data Polling
- DQ6: Toggle Bit

---

### 5.6 Chip-Erase

5555H → AAH  
2AAAH → 55H  
5555H → 80H  
5555H → AAH  
2AAAH → 55H  
5555H → 10H  

Dauer: ~50 ms

---

### 5.7 Produkt-ID

- Hersteller: DAh
- Gerät: C1h

---

## 6. Betriebsmodi

| Modus          | CE | OE | WE | Ergebnis |
|----------------|----|----|----|---------|
| Lesen          | L  | L  | H  | Daten |
| Schreiben      | L  | H  | L  | Schreiben |
| Standby        | H  | X  | X  | High-Z |

---

## 7. Elektrische Eigenschaften

- VDD: -0.5 bis +7 V
- Betrieb: 0–70 °C

---

## 8. Zeitparameter

- Lesen: 70–150 ns
- Schreiben: 10 ms (Page)

---

## 9. Hinweise

- Änderungen durch Hersteller möglich
- Validierung bei sicherheitskritischen Anwendungen erforderlich
