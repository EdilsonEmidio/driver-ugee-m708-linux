# Driver UGEE para Linux (Backend Rust + GUI GTK 4 / Libadwaita)

[![Linux](https://img.shields.io/badge/Linux-Debian%20%7C%20Ubuntu%20%7C%20Fedora%20%7C%20Arch-blue.svg)](#)
[![Display](https://img.shields.io/badge/Display-Wayland%20%7C%20X11-green.svg)](#)
[![Backend](https://img.shields.io/badge/Driver-Rust%20(evdev%20%2F%20uinput)-orange.svg)](#)
[![UI](https://img.shields.io/badge/GUI-GTK%204%20%2B%20Libadwaita-purple.svg)](#)

Driver *user-space* de alto desempenho em **Rust** combinado com painel de controle nativo em **Python + GTK 4 / Libadwaita / Cairo** para mesas digitalizadoras **UGEE 708M / Hanvon Ugee** no Linux (Wayland e X11).

---

## Recursos Principais

- **Daemon Rust de Baixa Latência (`ugee-driver`)**:
  - Leitura direta via `hidraw` + descritores USB (`0x64`, `0x6E`, `0x83`) com suporte a **33+ PIDs** (`M708 v1/v2/v3` incluindo `0x0907`, `0x0924`, `0x0927` de 16384 níveis, entre outros modelos UGEE).
  - Emulação `uinput` separada para **Caneta (`UGEE Tablet Stylus`)**, **Atalhos/Mouse (`UGEE Tablet ExpressKeys`)** e **Pad Nativo Wayland (`UGEE Tablet Pad`)**.
  - Tabela de consulta pré-computada (**LUT**) para curva de pressão com **1 ou múltiplos pontos de controle** (Bézier quadrática para 1 ponto, spline cúbica monótona para 2+ pontos).
  - Notificações na tela (**OSD** via `notify-send`) e serviço automático `systemd --user`.
- **Interface Gráfica Nativa (`ugee-gui`)**:
  - **Área de Trabalho**: recorte interativo da área ativa da mesa (com indicador em tempo real da caneta), região de atuação na tela, rotação (`0°`, `90°`, `180°` canhoto, `270°`) e **Modo de Precisão**.
  - **Caneta e Pressão**: curva de sensibilidade multi-pontos (`+ Adicionar Ponto` / `− Remover Ponto`, mínimo 1), quadro de teste de traço ao vivo e mapeamento livre dos **2 botões da caneta** (com presets, digitação manual ou botão **Capturar**).
  - **Teclas de Atalho**: LEDs ao vivo, atalhos globais Wayland, execução de comandos, botões nativos e **Painel Flutuante** acionado via IPC.
  - **Diagnóstico e Perfis**: telemetria ao vivo, contagem de pacotes, bateria/LED (quando suportado) e exportação/importação de perfis em JSON.

---

## Pré-requisitos (Em um Novo PC)

### 1. Toolchain Rust (`cargo`, `rustc` e compilador C `gcc`)

Instale o compilador C e o Rust via `rustup` (recomendado):

```bash
# Debian / Ubuntu / Pop!_OS / Linux Mint
sudo apt update && sudo apt install -y curl build-essential git

# Fedora
# sudo dnf install -y curl gcc git

# Arch Linux / Manjaro
# sudo pacman -S --needed base-devel git curl

# Instalar Rust (se ainda não tiver)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 2. Dependências da Interface Gráfica (GTK 4 + Libadwaita + `notify-send`)

O script `install.sh` tenta instalar automaticamente via `apt`, `dnf`, `pacman` ou `zypper`. Se preferir instalar antes:

- **Debian / Ubuntu / Mint**:
  ```bash
  sudo apt install -y python3-gi python3-gi-cairo gir1.2-gtk-4.0 gir1.2-adw-1 libnotify-bin
  ```
- **Fedora**:
  ```bash
  sudo dnf install -y python3-gobject gtk4 libadwaita libnotify
  ```
- **Arch Linux**:
  ```bash
  sudo pacman -S --needed python-gobject gtk4 libadwaita libnotify
  ```
- **openSUSE**:
  ```bash
  sudo zypper install -y python3-gobject typelib-1_0-Gtk-4_0 typelib-1_0-Adw-1 libnotify-tools
  ```

---

## Instalação (A partir do GitHub)

```bash
# 1. Clone o repositório
git clone https://github.com/SEU-USUARIO/ugee-linux-driver.git
cd ugee-linux-driver

# 2. Execute o instalador
chmod +x install.sh uninstall.sh gui/ugee_gui.py
./install.sh
```

O `install.sh` executa automaticamente:
1. Verifica o toolchain Rust e instala dependências GTK 4 / Libadwaita.
2. Compila `ugee-driver` em modo `--release`.
3. Instala os binários em `/usr/local/bin/ugee-driver` e `/usr/local/bin/ugee-gui` (assets em `/usr/local/share/ugee-driver/`).
4. Instala os ícones e o atalho no Menu de Aplicativos (**Mesa Digitalizadora UGEE**).
5. Configura as regras `udev` em `/etc/udev/rules.d/60-ugee.rules` e a definição `libwacom` em `/etc/libwacom/ugee-m708.tablet`.
6. Habilita e inicia o serviço `systemd --user ugee-driver.service`.

> **Importante após a 1ª instalação em um PC novo:**
> - Desconecte e reconecte o cabo USB da mesa para aplicar as regras `udev`.
> - Se o seu usuário acabou de ser adicionado ao grupo `input`, faça **logout e login** novamente na sessão.
> - Caso utilize o `OpenTabletDriver`, desative-o para evitar conflito de acesso ao dispositivo:
>   ```bash
>   systemctl --user stop opentabletdriver.service 2>/dev/null || true
>   systemctl --user disable opentabletdriver.service 2>/dev/null || true
>   ```

---

## Como Exportar Suas Configurações para Outro PC

Suas configurações ficam salvas automaticamente em:
- `~/.config/ugee-driver/config.toml`

Você também pode usar os botões **Exportar Perfil (.json)** e **Importar Perfil (.json)** na aba **Diagnóstico e Perfis** da interface gráfica (salvo em `~/ugee_tablet_profile_backup.json`), ou copiar diretamente o arquivo `~/.config/ugee-driver/config.toml` para o outro computador.

---

## Comandos Úteis (CLI)

```bash
# Abrir a interface gráfica
ugee-gui

# Inspecionar hardware conectado, descritores e dispositivos uinput
ugee-driver --probe

# Consultar status e telemetria do serviço em execução
ugee-driver --status

# Recarregar o arquivo ~/.config/ugee-driver/config.toml em runtime
ugee-driver --reload

# Acompanhar logs do driver em tempo real
journalctl --user -u ugee-driver.service -f
```

---

## Desinstalação

```bash
./uninstall.sh
```

---

## Licença

Distribuído sob licença **MIT OR Apache-2.0**. Os ícones e ilustrações da mesa digitalizadora são propriedade da Hanvon Ugee Technology Co., Ltd.
