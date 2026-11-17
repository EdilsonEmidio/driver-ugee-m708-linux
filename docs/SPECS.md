# Especificação Técnica do Driver UGEE (Engenharia Reversa — Fase 1)

Este documento consolida todas as descobertas técnicas extraídas por engenharia reversa a partir do instalador oficial do fabricante (`./ugee_Win_4.3.6.250305/UGEEWin_4.3.6.250305.exe`) com apoio do servidor MCP `rea` e análise de disassembly x86, visando a reimplementação nativa e open-source em **Rust** para **Linux** (user-space via `hidapi`/`rusb` + `evdev`/`uinput`).

> [!IMPORTANT]
> Nenhuma informação neste documento foi inventada. Qualquer campo, comando ou comportamento de hardware que dependa de verificação em dispositivo físico conectado (USB/BLE) está explicitamente marcado como **`TODO`**.

---

## 1. Metadados dos Artefatos e Evidências de Análise (MCP REA)

### 1.1 Contêiner e Arquivos Extraídos

O executável `ugee_Win_4.3.6.250305/UGEEWin_4.3.6.250305.exe` é um instalador **Inno Setup 5.5.0 (u)** compactado em blocos LZMA (chunks com CRC32 de 4 bytes + filtro BCJ `TransformCallInstructions` em `0x40f784`). Todos os arquivos relevantes foram extraídos e verificados contra os hashes SHA-1 embutidos no cabeçalho do instalador:

| Artefato | Tamanho (bytes) | SHA-1 (Verificado) | SHA-256 | Papel na Arquitetura Original |
| :--- | ---: | :--- | :--- | :--- |
| `UGEEWin_4.3.6.250305.exe` | 26.336.568 | — | `15a430a32f931de3b975bd969f3d687f872d4dbda7b9aa15580536a08c89e760` | Instalador Inno Setup 5.5.0 (u) (`hdr_off=0x1887ba3`, `data_off=0x3c600`) |
| `ugeeTabletDriver.exe` | 227.416 | `f03cd0e740b83e32a81c0f984c07ba80cf628e06` | `e08488c611a5b18b8b77b8d764d051f78928eb418d50a22672c43c5db5b90429` | Serviço/daemon principal (leitura HID/BLE, parser de pacotes, matemática, IPC) |
| `ugeeTablet.exe` | 898.648 | `5ad00717bf485b6f187f3950ab38f0c1de00e278` | `b576837ba65afb3ac7bb5e65a2d9c9d038f89f5408825d92e07e92749580c9b9` | Interface gráfica Qt5 (configuração de usuário, tray icon, teste de pressão) |
| `Ugee_Tablet.xml` | 73.257 | `b418c91c4e404f8631429cf6bfc1799906ab59fe` | `75c2b7c464ff1a615e85b0a0439e8202839b039f2e0f5e2d3876471c2a451d3d` | Base XML v1.4.1 com 34 perfis padrão de modelos e atalhos |
| `name_config.ini` | 1.199 | `6739cd8be6ea54e444c85fd3199169ebc5f6dab1` | `2f6f190030667194bee3ec0617823955e1f12de10fa3e9fe58f79e3e08f3c044` | Mapeamento de PIDs suportados (seções `[Common]` e `[Dongle]`) |
| `language.ini` | 121.373 | `5b0a198aa6535909d0d770b26cfbb6a8e9d04b91` | `f89bbf58f878386f5018123421004ae2f24f28914125d41769d5f31ba7e70aa2f` | Strings de UI multilíngues e tabela oficial de códigos `Actions.*` (`Actid`) |
| `Tipslanguage.ini` | 4.268 | `4d740745b314ab8677938d7138c7b2069b895205` | `9fd090b98fa90e58ac8b5018cc32a2ff952c67b0a115519c0669ea0db4fda2ba` | Dicas/mensagens OSD da UI |
| `vmulti.inf` | 4.032 | `99fbbda62b663a9b2609539c2ba8138370b27629` | `b145007459494cdb264e1ca8c501847c2406dcfbc03c06891a1ea2e0be9e2ef5` | Driver HID virtual Windows (`pentablet\hid` / `vmulti.sys` KMDF 1.9) |
| `hanvonugeemfilter.inf` | 6.366 | `a891f1b4db8b3128fafc67344e37b4abfcca2a07` | `0ed01aafce6532ba3cd19d025bfda37f6e8a06a5ef612c423f9dfa853580900b` | Filtro Upper Filter de mouse HID no Windows (`HID\PenTablet&Col01`) |

### 1.2 Evidências Registradas no MCP REA (`ugeeTabletDriver.exe`)

- **Formato / Arquitetura:** PE32 (`pei-i386`, x86, Base `0x00400000`, Entry Point `0x0041d969`)
- **Seções principais:**
  - `.text`: `0x00401000 .. 0x0042198a`
  - `.rdata`: `0x00422000 .. 0x0042e490`
  - `.data`: `0x0042f000 .. 0x0042f400`
  - `.rsrc`: `0x00430000`
  - `.reloc`: `0x00431000`
- **IDs de Evidência MCP `rea`:**
  - `open_binary`: SHA-256 `e08488c611a5b18b8b77b8d764d051f78928eb418d50a22672c43c5db5b90429`
  - `inspect_with_objdump` (`file_headers`): `ev_028df000077620e5fd2163d98173a01eeb6b27437097185dc48dcaf6c24e64a2`
  - `inspect_with_objdump` (`section_headers`): `ev_ed3384a8a17411878c5b9765ae54cae8e4e2b83ba2b1255812216194be071ed6`
  - `inspect_pe_resources`: `ev_7f7c1f176d4f1ff65ad76e8786131d723dae8733f066bdeb07fa83a703bd52fe`
  - `inspect_with_objdump` (`symbols`): `ev_76acb39548279e87fe2e1ee2ef9a9dd22f2901d9533d41b5e04a073b053ba4e0`

---

## 2. Identificação do Hardware, Interfaces USB/HID e Handshake

### 2.1 Vendor ID (VID) e Modos de Descoberta

- **Vendor ID (VID):** `0x28BD` (**Hanvon Ugee Technology Co., Ltd.**)
- **Padrões de match de dispositivo em `ugeeTabletDriver.exe`:**
  - **USB / HID:** `VID_28BD` e formato `vid_%04X&pid_%04X&mi_%02d`
  - **Bluetooth / BLE (via `HuBleLib.dll` no Windows):** `0228BD_PID` / `VID&02%04X_PID&%04X`

### 2.2 Product IDs (PIDs) Suportados

#### A) PIDs Listados em `name_config.ini` (`[Common]` e `[Dongle]`)

Todos os 27 PIDs abaixo constam simultaneamente nas seções `[Common]` e `[Dongle]` de `name_config.ini` (com pequena variação apenas no nome do `0x0928`):

| PID (Hex) | Nome em `[Common]` | Nome em `[Dongle]` |
| :---: | :--- | :--- |
| `0x000B` | `UD13` | `UD13` |
| `0x0078` | `Rainbow 160` | `Rainbow 160` |
| `0x0096` | `EX12` | `EX12` |
| `0x0803` | `Rainbow 170` | `Rainbow 170` |
| `0x080A` | `EXRAI Pro 12` | `EXRAI Pro 12` |
| `0x0905` | `绘影 EX08` | `绘影 EX08` |
| `0x0907` | `M708` | `M708` |
| `0x0924` *(validado em HW)* | `M708` (`UGTABLET M708`, rev. `UG901_BPU1006_V11`) | `M708` |
| `0x090A` | `UG Pro 16` | `UG Pro 16` |
| `0x090D` | `EXRAI pro 16` | `EXRAI pro 16` |
| `0x0918` | `漫影HS960` | `漫影HS960` |
| `0x0919` | `S640-E` | `S640-E` |
| `0x091A` | `EXRAI um 16` | `EXRAI um 16` |
| `0x091B` | `EXRAI Pro 22R` | `EXRAI Pro 22R` |
| `0x091D` | `UG Pro 22` | `UG Pro 22` |
| `0x091E` | `EXRAI pro 22` | `EXRAI pro 22` |
| `0x091F` | `EXRAI 12` | `EXRAI 12` |
| `0x0920` | `漫影H960` | `漫影H960` |
| `0x0921` | `S640W-E` | `S640W-E` |
| `0x0925` | `EX08 Pro` | `EX08 Pro` |
| `0x0928` | `EX08 S/SW` | `EX08 SW` |
| `0x092B` | `EXRAI pro 13.3` | `EXRAI pro 13.3` |
| `0x092C` | `绘影 Y160F` | `绘影 Y160F` |
| `0x092D` | `EXRAI Pro 24` | `EXRAI Pro 24` |
| `0x093B` | `UGEE U1200` | `UGEE U1200` |
| `0x093C` | `UGEE U1600` | `UGEE U1600` |
| `0x0940` | `EX08 Pro` | `EX08 Pro` |
| `0x900A` | `UG Pro 16` | `UG Pro 16` |

#### B) PIDs Adicionais Encontrados Diretamente no Código de `ugeeTabletDriver.exe`

| PID (Hex) | Endereço no Disassembly | Comportamento Específico Observado |
| :---: | :--- | :--- |
| `0x0074` | `0x4104d8` (`CTablet::ParseData`) | Inverte a direção CW/CCW na decodificação em quadratura do dial/anel (`0x00 -> 0x40 -> 0x80 -> 0xC0`) |
| `0x0201` | `0x410645` (`CTablet::ParseData`) | Formato legado de Express Keys com 1 byte por tecla (`0x4101e0`), anel mapeado em posições `29`/`30` |
| `0x0933` | `0x411b51` | Ao despertar/conectar (`buf[1]=0xF8, buf[2]=0x02, buf[3]=0x01`), envia consulta dupla de LED (`cmd=0` com `arg1=1` e `arg1=2`) |
| `0x0934` | `0x411b58` | Mesmo comportamento especial de consulta dupla de LED que `0x0933` |

> [!NOTE]
> Modelos comercializados com outros nomes ou revisões de firmware (ex.: `UGTABLET M708` com `PID 0x0924`, bem como séries Q6/Q8W, M808/M908, E12/E12PLUS, UE16) têm seus perfis casados em runtime pelo `ProductName` (`"M708"`) ou pela String Descriptor USB índice `4` (`ICVersion` / `TabletType` em `<PenTableLists>`, ex: `UG901_BPU1006_V11`), não dependendo exclusivamente de uma tabela estática de PIDs.

#### C) Os 34 Perfis de Modelos Presentes em `Ugee_Tablet.xml` (v1.4.1)

Extraídos diretamente da tag `<PenTableLists version="1.4.1">`:

| Modelo (Tag XML) | `DeviceRingNum` | `DeviceKeyNum` | `DevicePenType` | `DeviceTrackPad` | `DeviceControl` | `DeviceKeyDirection` | Tem `<SC>` (Calibração)? | Botões Caneta (`<Pen>`) |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :--- |
| `RB160` | 1 | 6 | 0 | 0 | 0 | — | Não | `PenBtn1`, `PenBtn2` |
| `BP1003W` | 1 | 6 | 11 | 0 | 0 | — | Sim | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `BPU1001` | 1 | 6 | 4 | 0 | 0 | — | Não | `PenBtn0`, `PenBtn2` |
| `BPU1002` | 0 | 8 | 0 | 0 | 1 | — | Não | `PenBtn1`, `PenBtn2` |
| `M708` | 0 | 8 | 0 | 0 | 0 | — | Não | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `BPU0908` | 0 | 4 | 5 | 0 | 0 | — | Não | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `BPH0908` | 0 | 4 | 3 | 0 | 0 | — | Não | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `BPU0610W` | 0 | 10 | 0 | 0 | 0 | 1 | Não | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `BPG1002L` | 0 | 8 | 8 | 0 | 1 | — | Não | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `BPG1002L2` | 0 | 8 | 11 | 0 | 1 | — | Não | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `BPU0605W` | 0 | 8 | 11 | 0 | 0 | — | Não | `PenBtn1`, `PenBtn2` |
| `BPG0611` | 0 | 10 | 0 | 0 | 0 | 1 | Não | `PenBtn1`, `PenBtn2` |
| `BPG1011` | 0 | 12 | 0 | 0 | 0 | 1 | Não | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `UD13` | 0 | 6 | 0 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `LPH1101F` | 1 | 6 | 4 | 0 | 0 | — | Sim | `PenBtn0`, `PenBtn1` |
| `LPU1503` | 1 | 8 | 12 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `LPU1501` | 0 | 6 | 11 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `LPU2202` | 2 | 20 | 12 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `P22Pro` | 0 | 0 | 11 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `LAU2202` | 0 | 16 | 11 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `LPU1102` | 1 | 8 | 12 | 0 | 0 | — | Sim | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `LPU1302F` | 1 | 8 | 12 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `LPU1505F` | 2 | 8 | 13 | 0 | 0 | — | Sim | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `LPU2402Q` | 2 | 20 | 12 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `LPU1211F` | 0 | 0 | 10 | 0 | 0 | — | Sim | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `LPU1611F` | 0 | 0 | 10 | 0 | 0 | — | Sim | `PenBtn0`, `PenBtn1`, `PenBtn2` |
| `LA1501F` | 0 | 8 | 11 | — | — | — | Sim | `PenBtn1`, `PenBtn2` |
| `Q8W` | 0 | 0 | 7 | 0 | 0 | — | Não | (apenas curva + `TablePC`/`RelativeCoords`) |
| `Q6` | 0 | 6 | 2 | 0 | 0 | 1 | Não | (apenas curva + `TablePC`/`RelativeCoords`) |
| `M808` | 0 | 8 | 1 | 0 | 0 | — | Não | `PenBtn1`, `PenBtn2` |
| `M908` | 1 | 8 | 1 | 0 | 0 | — | Não | `PenBtn1`, `PenBtn2` |
| `E12` | 0 | 8 | 1 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `E12PLUS` | 1 | 8 | 1 | 0 | 0 | — | Sim | `PenBtn1`, `PenBtn2` |
| `UE16` | 1 | 9 | 15 | 0 | 0 | — | Sim | `PenBtn0`, `PenBtn1`, `PenBtn2` |

### 2.3 Interfaces USB/HID e Critério de Seleção

Na rotina de abertura de dispositivo (`0x403ae0`), após abrir o handle HID com `CreateFileW` (`GENERIC_READ | GENERIC_WRITE`), o driver chama:
1. `HidD_GetPreparsedData` (`ds:0x42202c`)
2. `HidD_GetAttributes` (`ds:0x422030`)
3. `HidP_GetCaps` (`ds:0x422038`), obtendo `HIDP_CAPS` (incluindo `UsagePage`, `Usage`, `InputReportByteLength`, `OutputReportByteLength`).

As interfaces USB/HID presentes nesses dispositivos seguem a divisão multi-interface (`mi_00`, `mi_01`, `mi_02`):
- **Interface Vendor Raw Digitizer (`UsagePage == 0xFF0A`, `Usage == 0x0001`):** Interface proprietária (tipicamente `MI_02`) onde são enviadas as solicitações de handshake e recebidos os relatórios brutos (`0x02 ...`) de caneta, botões, anéis e bateria.
- **Interfaces HID Padrão (`UsagePage == 0x000D` Pen / `0x0001` Mouse/Keyboard):** Interfaces compatíveis padrão USB HID usadas antes do handshake; uma vez ativado o modo vendor raw, o hardware passa a emitir eventos na interface `0xFF0A`. No Linux, caso o kernel `hid-generic` faça bind de `/dev/input/event*` nas interfaces padrão enquanto o daemon emula via `uinput`, deve-se considerar o comportamento do grab exclusivo ou desabilitação (ver nota sobre `EVIOCGRAB` na arquitetura Linux).

> [!NOTE]
> **Validação em Hardware Real (`28bd:0924` — `UGTABLET M708`):**
> A interface `MI_02` (`/dev/hidraw7`) expõe o Report Descriptor de 36 bytes:
> `06 0a ff 09 01 a1 01 85 02 09 02 15 00 26 ff 00 75 08 95 09 81 02 09 03 15 00 26 ff 00 75 08 95 09 91 02 c0`
> - `UsagePage = 0xFF0A`, `Usage = 0x0001`, `Report ID = 0x02`
> - Input Report (`0x81 0x02`): `9` bytes + `1` byte Report ID = **`10` bytes** (`InputReportByteLength = 10`)
> - Output Report (`0x91 0x02`): `9` bytes + `1` byte Report ID = **`10` bytes** (`OutputReportByteLength = 10`)
>
> `TODO`: Confirmar em outras variantes Ugee (ex.: 24 bits X/Y ou BLE) se há firmwares usando `UsagePage == 0xFF0A` com `Usage != 0x0001` ou comprimento diferente (`12`/`14`/`64` bytes).

### 2.4 Consulta de Descritores (USB String Descriptors — `0x403d40 .. 0x403feb`)

Imediatamente após abrir a interface HID, a função `0x403d40` consulta os seguintes índices via `HidD_GetIndexedString(hDevice, index, buf, len)`:

1. **Índice `0x64` (`100`) — Capacidades Físicas da Mesa (`64` bytes lidos em `[ebp-0x44]`):**
   No Windows, `HidD_GetIndexedString` entrega no buffer de saída os bytes de payload da USB String Descriptor (já sem os 2 bytes de cabeçalho USB `[bLength, bDescriptorType=0x03]`). Designando esse payload de `b[0..11]`:

   | Payload Index | Stack Offset (`0x403d40`) | Campo Decodificado | Expressão / Fórmula Exata (Disassembly `0x403d93..0x403e13`) |
   | :---: | :--- | :--- | :--- |
   | `b[0], b[1], b[10]` | `[ebp-0x44]`, `[ebp-0x43]`, `[ebp-0x3a]` | `MaxX` (24 bits) | `(b[10] << 16) \| (b[1] << 8) \| b[0]` |
   | `b[2], b[3], b[11]` | `[ebp-0x42]`, `[ebp-0x41]`, `[ebp-0x39]` | `MaxY` (24 bits) | `(b[11] << 16) \| (b[3] << 8) \| b[2]` |
   | `b[6], b[7]` | `[ebp-0x3e]`, `[ebp-0x3d]` | `MaxPressure` (16 bits LE) | `(b[7] << 8) \| b[6]` |
   | `b[8], b[9]` | `[ebp-0x3c]`, `[ebp-0x3b]` | `Resolution` (LPI, 16 bits LE) | `(b[9] << 8) \| b[8]`; se `== 0`, fallback para `0x13D8` (**`5080` LPI**) |
   | *Derivado* | `[esi+0x7a8]` | `Width` (`0.01 inch`) | `(MaxX * 100) / Resolution` |
   | *Derivado* | `[esi+0x7ac]` | `Height` (`0.01 inch`) | `(MaxY * 100) / Resolution` |

   > [!NOTE]
   > **Validado em Hardware Real no Linux (`28bd:0924` `UGTABLET M708` via `USBDEVFS_CONTROL` em `/dev/bus/usb/001/004`):**
   > - `0x01`: `"UGTABLET"` | `0x02`: `"M708"` | `0x03`: `"00000000"` | `0x04`: `"UG901_BPU1006_V11"` | `0x05`: `"2023-07-26_01_Recognize Android"`
   > - `0x64` bruto (`14` bytes): `0e 03 6a c6 0a 77 08 00 ff 1f d8 13 00 00`
   >   - Payload `b[0..12]` (a partir de `raw[2]`): `[6a, c6, 0a, 77, 08, 00, ff, 1f, d8, 13, 00, 00]`
   >   - `MaxX = 0x00C66A = 50794`, `MaxY = 0x00770A = 30474`, `b[4] = 8` (Express Keys), `MaxPressure = 0x1FFF = 8191`, `Resolution = 0x13D8 = 5080 LPI`, dimensões derivadas `999 x 599` (`9.99" x 5.99"`).
   > - A API `usbfs` (`USBDEVFS_CONTROL`) ou `rusb::DeviceHandle::read_control` retorna os 2 bytes de cabeçalho `[bLength, 0x03]` + payload binário intacto sem corrupção de UTF-16LE.

2. **Índices `1..5` (Strings UTF-16LE, `64` wchar_t cada):**
   - Índice `1` -> `Manufacturer`
   - Índice `2` -> `ProductName`
   - Índice `3` -> `SerialNumber`
   - Índice `4` -> `ICVersion` / `TabletType` (usado para localizar o perfil correspondente em `Ugee_Tablet.xml`, ex: `LPU1503`, `M708`, `UG901_BPU1006_V11`, etc.)
   - Índice `5` -> `ICData`

3. **Índices `0x6E` (`110`) e `0x83` (`131`):**
   - **Validado em Hardware Real (`28bd:0924` `UGTABLET M708`):**
     - `0x6E` bruto (`11` bytes): `0b 03 00 10 00 01 08 00 00 00 03` -> payload `p[0..9] = [00, 10, 00, 01, 08, 00, 00, 00, 03]`.
     - `0x83` bruto (`6` bytes): `06 03 01 04 00 04` -> payload `q[0..4] = [01, 04, 00, 04]`.
   - O byte de capacidade em `payload[1]` (`0x10` na M708) corresponde à verificação `>= 0x10` em `0x4042ab` / `0x411fc7` (`[esi+0x7c5]`), selecionando `mode = 0x04` quando `>= 0x10` e `0x02` caso contrário.
   - Os bits `[esi+0x7c7] & 0x18` habilitam comandos de brilho de LED (`cmd=0` em `0x411d58`), e `[esi+0x7cb] & 0x08` habilita polling de bateria (`cmd=4` em `0x407ae8`/`0x411f4b`).
   - `TODO`: Confirmar em dispositivos com LED configurável e bateria sem fio (ex.: série Q8W, LPU2202, EXRAI) os bits correspondentes de `0x6E`/`0x83` para `[esi+0x7c7]` e `[esi+0x7cb]` (na M708 com cabo, os bytes após `p[1]` são `[00, 01, 08, 00, 00, 00, 03]` e `q = [01, 04, 00, 04]`).

### 2.5 Pacotes de Handshake / Output Reports Enviados à Mesa

O driver envia **Output Reports** usando `WriteFile` (ou API BLE `SendCmd` em modo Bluetooth) — **não** `HidD_SetFeature` — com o tamanho `HIDP_CAPS.OutputReportByteLength` (tipicamente `64` bytes, ou pacotes curtos de `10`/`12` bytes preenchidos com `0x00` no restante do buffer). Note que no Linux (`hidapi::write`), o primeiro byte `buf[0]` **é** o Report ID (`0x02`).

| Comando / Rotina | Endereço Disassembly | Tamanho do Buffer | Bytes Enviados (`buf[0..]`) | Condição / Finalidade |
| :--- | :--- | :---: | :--- | :--- |
| **Ativar Modo Raw (Init USB)** | `0x403ff0` | `OutputReportByteLength` | `[0x02, 0xB0, mode, 0x00, ...]` | Envia via `WriteFile` com timeout `300ms` (`0x12c`). Alterna o firmware para reports proprietários na interface `0xFF0A`. |
| **Ativar Modo Raw (Wake / `cmd=3`)** | `0x411fa0` / `0x407a70` / `0x411ea8` | `64` (`0x40`) ou `10` (`0x0A`) | `[0x02, 0xB0, mode, 0x00, ...]` | `mode = 0x04` se `[esi+0x7c5] >= 0x10`, senão `0x02`. Também reenviado ao receber report `0xF8 0x02 0x01` (wake). |
| **Alternar para Modo Android / Pass** | `0x407bb0` | `64` (`0x40`) | `[0x02, 0xB0, 0x04, 0x00, ...]` | Envia `mode = 0x04` fixo preenchido até 64 bytes com `0x00`. |
| **LED Brilho (`cmd=0`)** | `0x4079e4` / `0x411d54` | `12` (`0x0C`) | `[0x02, 0xB4, 0x01, 0x00, val, 0x00, ...]` | Ativo quando `([esi+0x7c7] & 0x18) != 0`. Em `PID 0x0933`/`0x0934` consulta com `val=1` e `val=2`. |
| **LED Brilho Duplo (`cmd=1`)** | `0x407a03` / `0x411d92` | `12` (`0x0C`) | `[0x02, 0xB4, 0x01, 0x01, val1, val2, 0x00, ...]` | Ajusta `TabletLightValue` / `WheelLightValue`. |
| **LED Modo/Valor (`cmd=2`)** | `0x407a2a` / `0x411e63` | `12` (`0x0C`) | `[0x02, 0xB4, 0x02, 0x01, 0x00, mode, arg2, 0x00, ...]` | Envia `arg2` em `buf[6]` quando `mode == 0x01`; requer `[esi+0x92a] != 0`. |
| **Poll de Bateria (`cmd=4`)** | `0x407ae3` / `0x411f47` | `10` (`0x0A`) | `[0x02, 0xB8, 0x04, 0x00, ...]` | Ativo quando `([esi+0x7cb] & 0x08) != 0`. |
| **Consultas Dongle / BLE (`cmd=11..19`)** | `0x407b08 .. 0x407b78` | `10` (`0x0A`) | `[0x02, 0x80, 0x06, sub_id, 0x00, ...]` | `sub_id` em `{0x02, 0x03, 0x04, 0x05, 0x64, 0x67, 0x6E, 0x83, 0xF1}`. |

> [!NOTE]
> `TODO`: Confirmar em dispositivo real sem fio / dongle 2.4GHz os significados exatos de resposta para `sub_id = 0x67` e `0xF1` no comando `0x02 0x80 0x06 <sub_id>` (já sabemos que `0x02..0x05`, `0x64`, `0x6E` e `0x83` espelham os String Descriptors equivalentes, conforme visto em `0x408246` para `sub_id == 0x64` onde `buf[3] == 0x01` indica sucesso).

---

## 3. Protocolo de Pacotes (Byte Layout)

A decodificação central ocorre em `CTablet::ParseData(raw_buf, len, out_struct)` no endereço **`0x410440`** (chamada por `0x411970` na linha `0x411bdf`). A struct de saída (`out_struct`, tamanho **`0x7C` = 124 bytes**) possui o seguinte layout verificado em disassembly:

| Offset Struct | Tipo | Campo | Origem / Descrição |
| :---: | :---: | :--- | :--- |
| `+0x00` | `u32` | `device_id` | Handle/ID do dispositivo (`[ebp+0x914]`) |
| `+0x04` | `u32` | `pid` | Product ID USB (`[ebp+0x910]`, usado para quirks `0x0074` e `0x0201`) |
| `+0x08` | `u16` | `event_flags` | Bitmask: `0x01`=Caneta, `0x02`=ExpressKey, `0x04`=TouchRing/Dial, `0x08`=Mudança botão caneta, `0x80`=Out-of-Range / desconhecido |
| `+0x0C` | `u32` | `x` | Coordenada X (16 bits ou 24 bits) |
| `+0x10` | `u32` | `y` | Coordenada Y (16 bits ou 24 bits) |
| `+0x14` | `u16` | `pressure` | Pressão bruta LE `(buf[7] << 8) \| buf[6]` (`+0x16` zerado) |
| `+0x18` | `i8` | `tilt_x` | Inclinação eixo X com sinal (preenchido se `len >= 10`) |
| `+0x19` | `i8` | `tilt_y` | Inclinação eixo Y com sinal (preenchido se `len >= 10`) |
| `+0x1A` | `u8` | `status` | Byte bruto `buf[1]` da caneta |
| `+0x1B` | `u8` | `pen_btn_changed_mask` | Máscara bits `0..3` dos botões da caneta que mudaram de estado |
| `+0x1C` | `u8` | `pen_btn_pressed` | `1` pressionado, `0` solto |
| `+0x20..+0x47` | `[u8; 40]` | `keys_current` | Estado atual (`0`/`1`) de até 40 Express Keys |
| `+0x48..+0x6F` | `[u8; 40]` | `keys_prev` | Estado anterior (`0`/`1`) das Express Keys (detectado em `0x410250`) |
| `+0x70` | `u16` | `ring_index` | Índice do anel/dial (`0` = Ring 1, `1` = Ring 2) |
| `+0x72` | `i16` | `ring_delta` | Direção de giro (`+1` horário / CW, `-1` anti-horário / CCW) |

### 3.1 Pré-filtros Antes de `CTablet::ParseData` (`0x411970 .. 0x411b9b`)

Antes de invocar `CTablet::ParseData`, a função receptora `0x411970` trata mensagens de sistema/controle baseadas em `buf[1]`:

1. **Resposta de LED (`buf[1] == 0xB4`, `buf[2] == 0x01`, `buf[3] in {0x00, 0x01}`):**
   - `buf[4] == 0x02`: Atualiza `TabletLightValue = buf[5]` (`WORD [ebp+0x84c]`)
   - `buf[4] == 0x01`: Atualiza `WheelLightValue = buf[5]` (`WORD [ebp+0x84e]`)
2. **Resposta de Bateria (`buf[1] == 0xF2`, endereços `0x4081c2` / `0x411aff`):**
   - No disassembly observado, ao receber `buf[1] == 0xF2`, o driver armazena diretamente `buf[3]` em `[ebp+0xc]` (`BATTERY` / nível de bateria).
   - `TODO`: Confirmar em dispositivo sem fio / dongle real o significado exato de `buf[2]`, a escala de `buf[3]` (`0..100` vs degraus discretos) e eventual byte indicador de carregamento (`buf[4]`).
3. **Evento de Wake / Reconexão (`buf[1] == 0xF8, buf[2] == 0x02, buf[3] == 0x01`):**
   - Se `[ebp+0x92c] != 0`, reenvia automaticamente o comando de modo raw (`cmd=3` com `[0x02, 0xB0, mode, ...]`).
   - Se `PID == 0x0933` ou `PID == 0x0934`, consulta os LEDs (`cmd=0` com `arg1=1` e `arg1=2`).

### 3.2 Pacotes de Caneta (Stylus — `0x410637 .. 0x41088b`)

Condição de entrada em `CTablet::ParseData`: `(buf[1] & 0xF0) == 0xA0` **ou** `buf[1] == 0xC0`.

| Byte Offset | Tamanho | Campo | Decodificação Exata (Disassembly `0x4106ae..0x41088b`) |
| :---: | :---: | :--- | :--- |
| `0` | 1 | `report_id` | Tipicamente `0x02` em modo vendor raw USB (`0xFF0A`). `CTablet::ParseData` (`0x410488`) apenas distingue `buf[0] == 0x06` dos demais pacotes, roteando a caneta pelo byte `buf[1]`. |
| `1` | 1 | `status` | Byte de estado da caneta e bits `0..3` de botões (`buf[1] & 0x0F` passado para `0x4103b0`). |
| `2..=3` (+`10`) | 2 ou 3 | `x` | Se `len != 12`: `(buf[3] << 8) \| buf[2]` (`u16` LE).<br>Se `len == 12`: `((buf[10] as u32) << 16) \| ((buf[3] as u32) << 8) \| (buf[2] as u32)` (24 bits). |
| `4..=5` (+`11`) | 2 ou 3 | `y` | Se `len != 12`: `(buf[5] << 8) \| buf[4]` (`u16` LE).<br>Se `len == 12`: `((buf[11] as u32) << 16) \| ((buf[5] as u32) << 8) \| (buf[4] as u32)` (24 bits). |
| `6..=7` | 2 | `pressure` | `(buf[7] << 8) \| buf[6]` (`u16` LE). |
| `8` | 1 | `tilt_x` | `buf[8] as i8` (somente lido quando `len >= 10`, `0x41087a`). |
| `9` | 1 | `tilt_y` | `buf[9] as i8` (somente lido quando `len >= 10`, `0x41087a`). |

#### Mapeamento do Byte `buf[1]` (`status` da Caneta)

Os 4 bits menos significativos (`buf[1] & 0x0F`) alimentam o detector de botões da caneta em `0x4103b0` (4 bits individuais testados em loop `0..4`):
- `0xA0`: Hover no alcance (Tip=0, Btn1=0, Btn2=0, Eraser=0)
- `0xA1`: Ponta tocando / Tip Down (`bit 0 = 1`)
- `0xA2`: Hover + Botão Lateral 1 (`bit 1 = 1`)
- `0xA3`: Tip + Botão Lateral 1 (`bits 0,1 = 1`)
- `0xA4`: Hover + Botão Lateral 2 (`bit 2 = 1`)
- `0xA5`: Tip + Botão Lateral 2 (`bits 0,2 = 1`)
- `0xA8`: Borracha Hover (`bit 3 = 1`, `0x40b703`)
- `0xA9`: Borracha Tip Down (`bits 0,3 = 1`, `0x40b70a`)
- `0xC0`: Pacote em modo relativo / out-of-range-to-relative (`0x40b9e6`, `0x410641`)
- Qualquer valor fora de `0xF0`, `(val & 0xF0) == 0xA0`, `0xC0` (e fora da exceção `PID=0x0201`): marca `event_flags = 0x80` (`Out-of-Range` / sair de proximidade, `0x41069e`) e retorna `false` (`0`). Por exemplo, `buf[1] == 0x00` ou `0x80` ou `0xE0`.

> [!NOTE]
> Nota sobre `len == 12`: O formato estendido de 12 bytes usa `buf[10]` como MSB (bits 16..23) de X e `buf[11]` como MSB de Y, exatamente espelhado na decodificação de `MaxX`/`MaxY` no String Descriptor `0x64` (bytes `b[10]` e `b[11]`).

### 3.3 Pacotes de Express Keys e Touch Ring / Dial (`buf[1] == 0xF0` ou `buf[0] == 0x06`)

Tratados em `0x410493 .. 0x410636` e helper `0x410250`:

#### A) Express Keys (Bitmask de até 40 teclas — `0x410250`)

- **Se `buf[0] == 0x06` (`0x410291 .. 0x410310`):** lê **4 bytes** de bitmask: `buf[1..=4]` (32 teclas: byte `k` contém teclas `8*(k-1)+1 .. 8*(k-1)+8`, ordem LSB para MSB `0x01 .. 0x80`).
- **Caso padrão (ex: `buf[0] == 0x02 && buf[1] == 0xF0`, `0x410312 .. 0x410385`):** lê **5 bytes** de bitmask: `buf[2..=6]` (**40 teclas**:
  - `buf[2]`: Teclas `K1 .. K8` (`bit 0` = `K1`, ..., `bit 7` = `K8`)
  - `buf[3]`: Teclas `K9 .. K16`
  - `buf[4]`: Teclas `K17 .. K24`
  - `buf[5]`: Teclas `K25 .. K32`
  - `buf[6]`: Teclas `K33 .. K40`).
- **Exceção legada `PID == 0x0201` (`0x410645`, helper `0x4101e0`):** Se `buf[1] != 0x01`, lê `buf[3..=19]` diretamente como array booleano `0/1` por tecla (`20` posições). As posições `keys_current[9]` e `keys_current[10]` acionam Touch Ring `ring_index=0`, `delta=+1` e `delta=-1` respectivamente.

#### B) Touch Ring / Dial (`buf[7]` quando `buf[1] == 0xF0` — `0x4104cf .. 0x410636`)

1. **Modo Discreto (comum na maioria dos modelos):**

   | Valor de `buf[7]` | `ring_index` | `ring_delta` | Endereço Disassembly |
   | :---: | :---: | :---: | :--- |
   | `0x01` | `0` (Ring 1) | `+1` (CW / Horário) | `0x410599 .. 0x4105a8` |
   | `0x02` ou `0xFF` | `0` (Ring 1) | `-1` (CCW / Anti-horário) | `0x4105ad .. 0x4105dc` |
   | `0x04` ou `0x10` | `1` (Ring 2) | `+1` (CW / Horário) | `0x4105e1 .. 0x410611` |
   | `0x08` ou `0x20` | `1` (Ring 2) | `-1` (CCW / Anti-horário) | `0x4105e5 .. 0x410632` |

2. **Modo Quadratura (`0x00 -> 0x40 -> 0x80 -> 0xC0 -> 0x00`, `0x4104cf .. 0x410593`):**
   - O driver mantém o valor anterior de `buf[7]` (`g_prev_dial`, endereço `0x42f16c`):
     - Transição `0x40 -> 0x00` ou `0x80 -> 0xC0`: `delta = -1` (ou `+1` se `PID == 0x0074`).
     - Transição `0x40 -> 0xC0` ou `0x80 -> 0x00`: `delta = +1` (ou `-1` se `PID == 0x0074`).
   - `TODO`: Hardware com 2 anéis em quadratura não é diferenciado nesse bloco (`ring_index` sempre `0`). Confirmar em modelos de 2 anéis (`LPU2202`, `LPU1505F`, `LPU2402Q`) quais valores exatos (`0x04/0x08` vs `0x10/0x20`) cada anel emite em hardware real.

### 3.4 Vetores de Bytes Reais / Conhecidos para Testes Unitários (Fase 2)

A tabela abaixo fornece casos de teste determinísticos derivados diretamente das instruções de `CTablet::ParseData` (`0x410440`) para validação unitária em `cargo test`:

| Cenário de Teste | Array de Bytes (`raw`) | Resultado Esperado |
| :--- | :--- | :--- |
| **Stylus 10 bytes — Hover** | `[0x02, 0xA0, 0x10, 0x27, 0x88, 0x13, 0x00, 0x00, 0xFC, 0x05]` | `flags=0x01`, `X=10000`, `Y=5000`, `pressure=0`, `tilt_x=-4`, `tilt_y=5`, `status=0xA0` |
| **Stylus 10 bytes — Tip + Btn1 + Tilt** | `[0x02, 0xA3, 0xFF, 0x7F, 0x00, 0x40, 0xFF, 0x1F, 0xE2, 0x1E]` | `flags=0x09` (caneta + mudança botão), `X=32767`, `Y=16384`, `pressure=8191`, `tilt_x=-30`, `tilt_y=30`, `btn_mask=0b0011` (`Tip`+`Btn1`) |
| **Stylus 12 bytes — 24-bit X/Y** | `[0x02, 0xA1, 0x40, 0x9C, 0x20, 0x4E, 0xFF, 0x3F, 0x00, 0x00, 0x01, 0x01]` | `flags=0x09`, `X=0x019C40` (`105536`), `Y=0x014E20` (`85536`), `pressure=16383` |
| **Stylus Out-of-Range** | `[0x02, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]` | Retorno `false`, `flags=0x80` (sinaliza saída de proximidade) |
| **Express Keys K1 e K10 pressionadas** | `[0x02, 0xF0, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]` | `flags=0x02`, `keys[0]=1` (`K1`), `keys[9]=1` (`K10`), demais `0` |
| **Express Keys modo `0x06` (K3)** | `[0x06, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]` | `flags=0x02`, `keys[2]=1` (`K3`) |
| **Touch Ring 1 CW (`0x01`) e CCW (`0xFF`)** | `[0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00]`<br>`[0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00]` | CW: `flags=0x04, ring=0, delta=+1`<br>CCW: `flags=0x04, ring=0, delta=-1` |
| **Touch Ring 2 CW (`0x04`) e CCW (`0x08`)** | `[0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00]`<br>`[0x02, 0xF0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00]` | CW: `flags=0x04, ring=1, delta=+1`<br>CCW: `flags=0x04, ring=1, delta=-1` |

---

## 4. Recursos, Matemática de Transformação e Configurações

### 4.1 Estrutura do Arquivo de Configuração (`Ugee_Tablet.xml` v1.4.1)

O arquivo de perfis fica embutido no pacote e é copiado para o diretório do usuário (`%APPDATA%\ugeeTablet\Ugee_Tablet.xml` no Windows). Cada tag de modelo dentro de `<PenTableLists version="1.4.1">` possui:
- **`<DeviceInfo>`:** `DeviceRingNum`, `DeviceKeyNum`, `DevicePenType`, `DeviceTrackPad`, `DeviceControl`, opcionalmente `DeviceKeyDirection`.
- **`<Common>`:**
  - `<ScreenMapping id="0"><SX/><SY/><SW/><SH/></ScreenMapping>`: Monitor alvo (`id`) e retângulo de tela (`SX, SY, SW, SH` em pixels; `0,0,0,0` = monitor inteiro).
  - `<TabletPannel Orientation="0"><TX/><TY/><TW/><TH/></TabletPannel>`: Rotação (`0..3`) e sub-área ativa da mesa (`TX, TY, TW, TH` em `0.01 inch`; se `TW == 0 || TH == 0`, usa a área física total `Width × Height` do String Descriptor `0x64`).
  - Flags globais: `<DisableInfo>`, `<DisableQuickKey>`, `<DisablePressure>`, `<DisableSlope>`.
  - `<SC fKX="1" fXLC="1" fKY="1" fYLC="1"/>` (presente em displays gráficos): parâmetros de calibração de paralaxe.
  - `<PictureFiles>`: retângulos da imagem usada pela GUI por orientação (`<T_0>`, `<T_1>`, `<T_2>`, `<T_3>`).
- **`<CommonAPP>`** e **`<Apps>/<app id="0" name="0">`** (perfis globais e por aplicativo):
  - `<Pen>`: `<PenBtn0>`, `<PenBtn1>`, `<PenBtn2>` (com atributos `id` e `Actid`), curva de pressão `<P0>`, `<P1X>`, `<P1Y>`, `<P2>`, `<TablePC>` (Windows Ink `1/0`), `<RelativeCoords Speed="5">0</RelativeCoords>`.
  - `<K>`: elementos `<K1 Motid="1" Show="1" id="0" Actid="..."/>` até `K20`.
  - `<R id="0">` e `<R2 mode="0" id="0">`: grupos de funções `<W1>` .. `<W4>` alternáveis por `Actid=405` / `Actid=406`.
- **`<DesktopShortcutKeys>`:** Grade OSD flutuante (`<Fixed>`, `<Zoom>`, `<DisplayedAtCursor>`, `<Rows>`, `<Columns>`, `<VirtualKey>/<K1..K16>`).

### 4.2 Matemática de Rotação de Tela (`0x40b370`: `0x40b3d1 .. 0x40b5ea`)

Sendo $W_{\max}$ (`MaxX`) e $H_{\max}$ (`MaxY`) os limites brutos da mesa e `Orientation` $\in \{0, 1, 2, 3\}$:

| `Orientation` | Ângulo | Coordenada $X'$ | Coordenada $Y'$ | $\text{TiltX}'$ | $\text{TiltY}'$ | Área Máxima Efetiva |
| :---: | :---: | :--- | :--- | :---: | :---: | :--- |
| `0` | 0° (Padrão) | $X$ | $Y$ | $\text{TiltX}$ | $\text{TiltY}$ | $(W_{\max}, H_{\max})$ |
| `1` | 90° Horário | $H_{\max} - Y$ | $X$ | $-\text{TiltY}$ | $\text{TiltX}$ | $(H_{\max}, W_{\max})$ |
| `2` | 180° | $W_{\max} - X$ | $H_{\max} - Y$ | $-\text{TiltX}$ | $-\text{TiltY}$ | $(W_{\max}, H_{\max})$ |
| `3` | 270° Horário | $Y$ | $W_{\max} - X$ | $\text{TiltY}$ | $-\text{TiltX}$ | $(H_{\max}, W_{\max})$ |

Quando `DisableSlope != 0` (`[edi+0x76] != 0`), o driver zera explicitamente `TiltX = 0` e `TiltY = 0` (`0x40b768`).

### 4.3 Calibração Linear de Paralaxe (`<SC>`, `0x40b5ea .. 0x40b703`)

Quando ativa (`[ebx+0x78c] == 2` em Pen Displays com tag `<SC fKX fXLC fKY fYLC/>`), o driver corrige as coordenadas rotacionadas ao redor do ponto médio da área de trabalho da mesa $(W_{\text{work}}, H_{\text{work}})$ antes do mapeamento para tela:

$$X_{\text{cal}} = \left\lfloor \frac{X' - \lfloor W_{\text{work}} / 2 \rfloor}{f_{KX}} + 0.5 \cdot f_{XLC} \cdot W_{\text{work}} \right\rfloor$$

$$Y_{\text{cal}} = \left\lfloor \frac{Y' - \lfloor H_{\text{work}} / 2 \rfloor}{f_{KY}} + 0.5 \cdot f_{YLC} \cdot H_{\text{work}} \right\rfloor$$

Com valores padrão (`fKX = fKY = fXLC = fYLC = 1.0`), resulta exatamente em $X_{\text{cal}} = X'$ e $Y_{\text{cal}} = Y'$.

### 4.4 Área Ativa, Modo de Precisão e Modo Relativo (Mouse)

1. **Mapeamento Tablet Area (`TX, TY, TW, TH`) → Screen Mapping (`SX, SY, SW, SH`):**
   - `TX, TY, TW, TH` em centésimos de polegada são convertidos para unidades brutas usando `Resolution` (`(TX * Resolution) / 100` via divisão mágica em `0x40b774..0x40b7a6` e `(TW * MaxX) / Width_in` em `0x40b3f7..0x40b429`).
   - A coordenada dentro da área da mesa é verificada contra o retângulo ativo (`PtInRect`); se fora da área ativa, o pacote é ignorado com `status = 0x00` mantendo a última coordenada válida (`0x40b81e .. 0x40b850`).
   - Em seguida é feito o recorte e reescalamento linear para a área do monitor destino (`0..32767` para HID virtual Windows / espaço absoluto `0..Max` no Linux `evdev`).

2. **Modo de Precisão (`Actid = 404`):**
   - `TODO`: Confirmar no disassembly fora de `0x40b370` o fator exato de redução aplicado ao alternar `Actid=404` ("Precision mode"), para evitar suposições.

3. **Modo Relativo / Mouse (`<RelativeCoords Speed="...">1</RelativeCoords>`, `0x40b954 .. 0x40bc99`):**
   - Armazena a âncora inicial $(X_{\text{prev}}, Y_{\text{prev}})$ no primeiro pacote ao tocar/entrar na região (`0x40bc1f`).
   - Acumula deslocamento e resto sub-pixel entre pacotes:
     $$\text{accum}_x = (X - X_{\text{prev}}) + \text{rem}_x, \qquad \text{denom} = 5 \times (10 - \text{Speed})$$
     $$\Delta X = \text{trunc}\left(\frac{\text{accum}_x}{\text{denom}}\right), \qquad \text{rem}_x' = \text{accum}_x \bmod \text{denom}$$
   - Instrução exata verificada em `0x40ba31`: `mov ecx, 0xa; sub ecx, edx; lea ecx, [ecx+ecx*4]; idiv ecx`. Se `|\Delta X| > 0`, atualiza `rem_x` e emite movimento relativo $\Delta X$; caso contrário preserva o acumulador sem perder frações de movimento!

### 4.5 Curvas de Pressão (`P0, P1X, P1Y, P2` — `0x40c062 .. 0x40c245`)

A curva de sensibilidade de pressão é definida pelos pontos `<P0>`, `<P1X>`, `<P1Y>` e `<P2>` (padrão linear: `P0=0.0, P1X=0.5, P1Y=0.5, P2=1.0`), modelando uma **Curva Bézier Quadrática** sobre pressão normalizada $x = \text{Pressure} / P_{\max} \in [0, 1]$:

- Pontos de controle no plano normalizado:
  $$B_0 = (P_0, 0), \qquad B_1 = (P_{1X}, 1 - P_{1Y}), \qquad B_2 = (P_2, 1)$$
  *(Nota: $Y$ é invertido com `1.0 - P1Y` em `0x40c09c` porque na UI gráfica o eixo Y cresce para baixo).*
- Dada a pressão normalizada $x$, resolve-se $x(t) = (1-t)^2 P_0 + 2(1-t)t P_{1X} + t^2 P_2$:
  - Caso não-linear ($a = P_0 - 2 P_{1X} + P_2 \ne 0$):
    $$t = \operatorname{clamp}_{[0, 1]}\!\left(\frac{(P_0 - P_{1X}) + \sqrt{\max\!\left(0,\; (P_{1X} - P_0)^2 + (x - P_0) \cdot a\right)}}{a}\right)$$
  - Caso linear ($a \approx 0$, `0x40c13d`):
    $$t = \operatorname{clamp}_{[0, 1]}\!\left(\frac{x - P_0}{2 (P_{1X} - P_0)}\right)$$
- Pressão de saída normalizada e reescalada:
  $$y(t) = \operatorname{clamp}_{[0, 1]}\!\left(2(1 - t)t (1 - P_{1Y}) + t^2\right)$$
  $$\text{Pressure}_{\text{out}} = \begin{cases} \lfloor y(t) \cdot P_{\max} \rfloor & \text{se } P_{\max} > 8191 \\ \lfloor y(t) \cdot 8191 \rfloor & \text{se } P_{\max} \le 8191 \end{cases}$$
- Se `DisablePressure != 0` (`[edi+0x70] != 0` em `0x40c234`), o driver fixa a pressão de saída igual ao máximo (`MaxPressure`) sempre que `Pressure > 0`.

### 4.6 Tabela Completa de Ações / Atalhos (`Actid` de `language.ini` `[English]`)

Extraída integralmente de `language.ini` linhas 498–567:

| `Actid` | Categoria | Ação Original (EN) | Mapeamento Equivalente Sugerido (Linux / `evdev`) |
| :---: | :--- | :--- | :--- |
| `1` | Geral | Pen/Eraser | Alternar ponta Caneta ↔ Borracha |
| `10` | Geral | No Function | Nenhuma ação (`noop`) |
| `11` | Geral | Customize | Executar macro/teclas customizadas do nó XML |
| `12` | Geral | Default | Restaurar ação padrão do botão |
| `201..205` | Mouse/Mod | Shift, Left Alt, Right Alt, Ctrl, Space | `KEY_LEFTSHIFT`, `KEY_LEFTALT`, `KEY_RIGHTALT`, `KEY_LEFTCTRL`, `KEY_SPACE` |
| `206..209` | Mouse | Left click, Right click, Scroll click, Left double-click | `BTN_LEFT`, `BTN_RIGHT`, `BTN_MIDDLE`, duplo `BTN_LEFT` |
| `210, 211` | Mouse | Scroll up, Scroll down | `REL_WHEEL +1`, `REL_WHEEL -1` |
| `301..328` | Teclado | `301`: B, `302`: E, `303`: Alt, `304`: Space, `305`: Ctrl+S, `306`: Ctrl+Z, `307`: Ctrl+Alt+Z, `308`: Ctrl+Shift+Z, `309`: V, `310`: L, `311`: Ctrl+O, `312`: Ctrl+N, `313`: Ctrl+Shift+N, `314`: Ctrl+E, `315`: F, `316`: D, `317`: X, `318`: Ctrl+Delete, `319`: Ctrl+C, `320`: Ctrl+V, `321`: Ctrl++, `322`: Ctrl+-, `323`: Tab, `324`: F5, `325`: ], `326`: [, `327`: Ctrl+], `328`: Ctrl+[ | Sequências combinadas de teclas `KEY_*` via `uinput` |
| `401` | Dispositivo | Display/Hide Driver Interface | Notificar UI via IPC para mostrar/ocultar janela |
| `402` | Dispositivo | Switch monitor | Ciclar `ScreenMapping` entre monitores conectados |
| `403` | Dispositivo | Pen/Eraser | Alternar modo caneta/borracha |
| `404` | Dispositivo | Precision mode | Alternar modo de precisão (zoom de área ativa) |
| `405` | Dispositivo | Switch ring1 function | Ciclar função ativa (`W1..W4`) no Ring 1 |
| `406` | Dispositivo | Switch ring2 function | Ciclar função ativa (`W1..W4`) no Ring 2 |
| `407` | Dispositivo | Switch trackpad mode | Alternar modo Trackpad / Roda (`Mouse` ↔ `Roller`) |
| `408` | Dispositivo | `[B]/[E]` | Alternar entre tecla `B` e tecla `E` |
| `409` | Dispositivo | Eraser | Manter modo borracha enquanto pressionado |
| `410` | Dispositivo | Desktop shortcut keys | Exibir grade flutuante OSD (`DesktopShortcutKeys`) |
| `5` | Sistema | Run Application | Executar comando externo (no Linux: via UI/sessão do usuário, nunca como root) |
| `601..605` | Sistema | `601`: Lock Screen, `602`: Shut Down, `603`: Hibernate, `604`: Task View, `605`: Start Menu | Comandos D-Bus / XDG / `KEY_LEFTMETA` |
| `701..706` | Multimídia | `701`: Previous, `702`: Next, `703`: Play/Pause, `704`: Volume +, `705`: Volume -, `706`: Mute | `KEY_PREVIOUSSONG`, `KEY_NEXTSONG`, `KEY_PLAYPAUSE`, `KEY_VOLUMEUP`, `KEY_VOLUMEDOWN`, `KEY_MUTE` |

---

## 5. Fluxo da Interface Visual (GUI) e Comunicação com o Serviço

### 5.1 Funcionalidades Expostas na GUI (`ugeeTablet.exe` / `language.ini`)

A interface gráfica original é escrita em **Qt 5** e expõe 4 áreas principais de interação + ferramentas auxiliares:

1. **Área de Trabalho (`WorkArea`):**
   - Seleção de monitor alvo (`All monitor` ou monitor específico `UgeeScreenIndex`), área da tela (`Full screen area` ou seleção manual de 2 cantos `Set screen area`), proporção automática (`Screen proportion`).
   - Área ativa da mesa (`Full tablet area` ou seleção de 2 cantos com a caneta `Set tablet area`), ajuste manual dos cantos e **Rotação** (`0°`, `90°`, `180°`, `270°`).
   - **Calibração de Tela (`Calibrate`):** apenas em Pen Displays (`<SC>`), calcula os fatores `fKX, fXLC, fKY, fYLC`.
   - **Configuração de Display (`Display Setting`):** brilho, contraste, temperatura de cor (`Red`, `Green`, `Blue`) — `TODO`: verificar se é DDC/CI ou comando vendor específico (não está em `CTablet::ParseData`).
2. **Caneta (`Stylus` / `Pen`):**
   - Mapeamento dos botões `PenBtn0`, `PenBtn1`, `PenBtn2`.
   - **Curva de pressão visual interativa** (`P0, P1X, P1Y, P2` com slider `Soft` ↔ `Hard`) + área de teste ao vivo (`Pressure Test`, `Current pressure`).
   - Toggles: `Disable pressure`, `Disable tilt`, `Mouse mode` (`RelativeCoords` + `Mouse speed`), `Windows Ink` (`TablePC`).
3. **Teclas de Atalho & Anéis (`Shortcut key settings` / `Roller/Touch`):**
   - Gravação de até **20 combinações de teclas** por botão, escolha de `Actid` (mouse, teclado, sistema, multimídia, dispositivo, executar app), nome customizado.
   - Configuração de `R` (`W1..W4` com pares `KL`/`KR` para CCW/CW), `R2` e ajuste de luminosidade do LED (`TabletLightValue`, `WheelLightValue`).
   - Perfis independentes por aplicativo ativo (`<Apps>` em `Ugee_Tablet.xml`, trocados por monitoramento da janela em primeiro plano via `AppForegroundChangeCallback`).
4. **Configurações Gerais & Diagnóstico (`Setting` / `Diagnostic tool`):**
   - `Import config` / `Export config` (arquivos `.ugeecfg`), `Default`, ativação de OSD (`Message`), atalhos (`Shortcut keys`), janela de diagnóstico ao vivo (`Current X/Y`, pressão, botão, firmware/ICVersion, SerialNumber, status de bateria `Common.battery`).

### 5.2 Mecanismo de IPC no Windows vs. Arquitetura Alvo no Linux

#### No Windows (`ugeeTablet.exe` ↔ `ugeeTabletDriver.exe`)

A análise de strings e disassembly em `ugeeTabletDriver.exe` revelou **dois mecanismos complementares** de IPC:

1. **Canal de Controle Bidirecional (Socket Local Qt):**
   - O driver instancia `CLocalSocketServer` (`QLocalServer` com nome **`UgeeTabletLocalSocket`**, além de fallback `QTcpServer` em loopback).
   - O protocolo utiliza objetos **JSON** (`QJsonDocument` / `QJsonObject`) contendo chaves observadas no `.rdata`: `type`, `tabletId`, `msgId`, `msgData`, `appPath`, `timestamp`.
   - Comandos identificados: `ChangeTabletMode`, `ChangeTabletPass`, `SentTabletLightData` (`TabletLightValue`, `WheelLightValue`), `SetAreaOk`, `SentMultiData`, `ReceivActionMsg`, `OnShowInfo`.
   - **Recarregamento de Config:** A GUI grava as alterações no XML (`Ugee_Tablet.xml`) protegido por `QtLockedFile` (`CreateMutexW` / `QtLockedFile mutex `) e envia comando pelo socket local para o driver aplicar.

2. **Canal de Telemetria em Tempo Real (Memória Compartilhada):**
   - Objeto `QSharedMemory` (`tagUGShareMemoryData`) sincronizado pelo semáforo `TabletdataSem`.
   - Contém atributos da mesa conectada (`m_DeviceAttributes.*`) e buffer atualizado em tempo real a cada pacote em `0x411c7b..0x411cb3` (cópia da struct decodificada de `0x7C` bytes e coordenadas mapeadas + envio via `MULTIDRDATA` para a janela de diagnóstico e teste de pressão da GUI).

3. **Camada de Kernel/Sistema Virtual Windows (NÃO portar):**
   - `CPentabletDriver::EnmuVisualHidDevice` abre `vmulti.sys` (`pentablet\hid`), enquanto `hanvonugeemfilter.sys` filtra eventos redundantes de mouse, e `Local\WINTAB_MAP_FILENAME` alimenta WinTab32.

#### Arquitetura Implementada no Linux (User-Space Rust)

Seguindo [AGENTS.md](file:///home/edilson/Projetos/ugee-driver2/AGENTS.md):
- **Daemon (`ugee-driver`, [src/main.rs](file:///home/edilson/Projetos/ugee-driver2/src/main.rs)):** lê pacotes diretamente da interface `0xFF0A` (`Usage == 0x0001`) via `hidraw`/`rusb` (com regra `udev` em [udev/60-ugee.rules](file:///home/edilson/Projetos/ugee-driver2/udev/60-ugee.rules) para operação sem root), executa o pipeline de transformação (`ParseData` -> Rotação -> Calibração -> Área Ativa -> Modo Relativo/Precisão -> Curva Bézier) e emite eventos via **`evdev`/`uinput`** (`src/uinput/mod.rs`):
  - Dispositivo virtual 1 (**Stylus**): `ABS_X`, `ABS_Y`, `ABS_PRESSURE`, `ABS_TILT_X`, `ABS_TILT_Y`, `BTN_TOOL_PEN`, `BTN_TOOL_RUBBER`, `BTN_TOUCH`, `BTN_STYLUS`, `BTN_STYLUS2`, `INPUT_PROP_DIRECT` (ou `INPUT_PROP_POINTER` em mesas opacas).
  - Dispositivo virtual 2 (**Pad / Teclado / Mouse**): `BTN_0..` / `KEY_*` e `REL_WHEEL` / `REL_X` / `REL_Y` para Express Keys, Touch Ring e Modo Relativo.
- **Configuração e IPC Desacoplados ([src/config/mod.rs](file:///home/edilson/Projetos/ugee-driver2/src/config/mod.rs), [src/ipc.rs](file:///home/edilson/Projetos/ugee-driver2/src/ipc.rs)):**
  - Arquivo **TOML** XDG (`$XDG_CONFIG_HOME/ugee-driver/config.toml`, com fallback para `~/.config/ugee-driver/config.toml`) carregado automaticamente pelo daemon e persistido pela GUI.
  - Servidor IPC JSON por linha sobre Unix Domain Socket (`$XDG_RUNTIME_DIR/ugee-driver.sock`, fallback `/tmp/ugee-driver.sock`) em thread dedicada (`spawn_ipc_thread`) com os comandos `GetStatus`, `UpdateConfig`, `ReloadConfig` e `SetLedBrightness`, expondo `DaemonSnapshot`/`LiveTelemetry` e aplicando alterações em tempo real (*hot-reload*) sem acoplar a interface ao loop USB.
- **Interface Gráfica Nativa em Rust (`egui` / `eframe`, [src/gui.rs](file:///home/edilson/Projetos/ugee-driver2/src/gui.rs), [src/bin/ugee-gui.rs](file:///home/edilson/Projetos/ugee-driver2/src/bin/ugee-gui.rs)):**
  - Executável dedicado `ugee-gui` (ou `ugee-driver --gui`), além dos subcomandos CLI `--status` e `--reload`, comunicando apenas via Unix Domain Socket + TOML.
  - Abas implementadas: Área de Trabalho & Rotação (com pré-visualização ao vivo), Caneta & Curva de Pressão Bézier interativa (`P1X`/`P1Y`), Express Keys `K1..K8` & Touch Ring (presets + combinações customizadas de teclas) e Diagnóstico ao vivo + área de desenho para testar pressão em tempo real.

---

## 6. Lista Consolidada de `TODO`s (Validação Pendente com Hardware Real)

1. **String Descriptor `0x64` e `0x6E` no Linux (VALIDADO em `28bd:0924` `M708`):** Confirmado via `USBDEVFS_CONTROL` em `/dev/bus/usb/001/004` que `0x64` retorna `[0e, 03] + 12 bytes` (`50794x30474`, `8191`, `5080 LPI`, `b[4]=8` Express Keys) e `0x6E` retorna `[0b, 03] + 9 bytes` com `payload[1] = 0x10` (selecionando handshake `0x04`). `TODO`: Confirmar em outros modelos com LED/bateria (ex.: série Q8W, EXRAI, LPU2202) os bytes exatos que alimentam `[esi+0x7c7] & 0x18` e `[esi+0x7cb] & 0x08`.
2. **`TODO` (Touch Ring 2 e Quadratura):** Validar em hardware físico com 2 dials (`LPU2202`, `LPU1505F`, `LPU2402Q`) quais valores de `buf[7]` correspondem a CW/CCW do Ring 2 (`0x04/0x08` vs `0x10/0x20`).
3. **`TODO` (Consultas Dongle/BLE `0x02 0x80 0x06 <sub_id>`):** Validar respostas reais de `sub_id = 0x67` e `0xF1`, além da escala exata de `buf[3]` e flags de carregamento `buf[4]` no pacote de bateria `0xF2` e status Bluetooth LE no BlueZ/Linux.
4. **`TODO` (Precision Mode `Actid=404` e Display DDC/CI):** Inspecionar ou testar em execução real o fator exato de redução espacial do `Actid=404` e o controle de brilho/cor do painel LCD (DDC/CI vs protocolo USB).
5. **`TODO` (Supressão de Interfaces HID Padrão no Linux):** Testar com dispositivo conectado se é necessário `EVIOCGRAB` nas interfaces USB HID padrão (`MI_00`/`MI_01`) para evitar eventos duplicados com o dispositivo `uinput` criado pelo daemon.
