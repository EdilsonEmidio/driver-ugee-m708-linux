#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== Instalador do Driver Rust + GUI Python (GTK4/Libadwaita) UGEE ==="
echo ""

# 1. Verifica dependências de compilação Rust e runtime GTK4/Libadwaita
echo "[1/7] Verificando dependências do sistema..."
if [ -f "${HOME}/.cargo/env" ]; then
    # shellcheck disable=SC1091
    source "${HOME}/.cargo/env"
fi

if ! command -v cargo &>/dev/null; then
    echo "ERRO: 'cargo' (toolchain Rust) não encontrado."
    echo "Instale o Rust via: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "Ou pelo gerenciador de pacotes da sua distribuição (ex: sudo apt install cargo rustc)."
    exit 1
fi

if ! command -v cc &>/dev/null && ! command -v gcc &>/dev/null; then
    echo "ERRO: compilador/linker C ('gcc' ou 'cc') não encontrado."
    echo "Instale o pacote base de compilação (ex: build-essential no Debian/Ubuntu ou base-devel no Arch)."
    exit 1
fi

NEEDS_DEPS=false
if ! python3 -c "import gi; gi.require_version('Gtk', '4.0'); gi.require_version('Adw', '1')" &>/dev/null; then
    NEEDS_DEPS=true
fi

if [ "$NEEDS_DEPS" = true ]; then
    echo "Instalando dependências GTK 4 / Libadwaita / notify-send..."
    if command -v apt-get &>/dev/null; then
        sudo apt-get update && sudo apt-get install -y python3-gi python3-gi-cairo gir1.2-gtk-4.0 gir1.2-adw-1 libnotify-bin
    elif command -v dnf &>/dev/null; then
        sudo dnf install -y python3-gobject gtk4 libadwaita libnotify
    elif command -v pacman &>/dev/null; then
        sudo pacman -S --needed --noconfirm python-gobject gtk4 libadwaita libnotify
    elif command -v zypper &>/dev/null; then
        sudo zypper install -y python3-gobject typelib-1_0-Gtk-4_0 typelib-1_0-Adw-1 libnotify-tools
    else
        echo "Aviso: Gerenciador de pacotes não reconhecido. Instale manualmente python3-gobject, gtk4, libadwaita e libnotify."
    fi
else
    echo "Dependências Python / GTK4 / Libadwaita já estão instaladas."
fi

# 2. Compila o daemon Rust em modo release
echo "[2/7] Compilando daemon Rust (ugee-driver) em modo release..."
cargo build --release --manifest-path "${SCRIPT_DIR}/Cargo.toml" --bin ugee-driver

# 3. Instala binários, GUI Python e assets
echo "[3/7] Instalando binário Rust e interface gráfica Python..."
sudo install -m 0755 "${SCRIPT_DIR}/target/release/ugee-driver" /usr/local/bin/ugee-driver

sudo mkdir -p /usr/local/share/ugee-driver/gui/assets
sudo cp "${SCRIPT_DIR}/gui/ugee_gui.py" /usr/local/share/ugee-driver/gui/ugee_gui.py
sudo cp "${SCRIPT_DIR}/gui/config_adapter.py" /usr/local/share/ugee-driver/gui/config_adapter.py
sudo chmod 0755 /usr/local/share/ugee-driver/gui/ugee_gui.py
sudo chmod 0644 /usr/local/share/ugee-driver/gui/config_adapter.py
if [ -d "${SCRIPT_DIR}/gui/assets" ]; then
    sudo cp -r "${SCRIPT_DIR}/gui/assets/"* /usr/local/share/ugee-driver/gui/assets/
fi
sudo ln -sf /usr/local/share/ugee-driver/gui/ugee_gui.py /usr/local/bin/ugee-gui

# 4. Instala ícones e atalho no Menu de Aplicativos
echo "[4/7] Instalando ícones e atalho no menu de aplicativos..."
for sz in 16 32 48 64 128 256 512; do
    if [ -f "${SCRIPT_DIR}/gui/assets/icons/${sz}x${sz}/ugee-m708.png" ]; then
        mkdir -p "${HOME}/.local/share/icons/hicolor/${sz}x${sz}/apps"
        cp "${SCRIPT_DIR}/gui/assets/icons/${sz}x${sz}/ugee-m708.png" "${HOME}/.local/share/icons/hicolor/${sz}x${sz}/apps/ugee-m708.png"
        sudo mkdir -p "/usr/share/icons/hicolor/${sz}x${sz}/apps"
        sudo cp "${SCRIPT_DIR}/gui/assets/icons/${sz}x${sz}/ugee-m708.png" "/usr/share/icons/hicolor/${sz}x${sz}/apps/ugee-m708.png"
    fi
done

if [ -f "${SCRIPT_DIR}/gui/assets/icon.png" ]; then
    mkdir -p "${HOME}/.local/share/pixmaps"
    cp "${SCRIPT_DIR}/gui/assets/icon.png" "${HOME}/.local/share/pixmaps/ugee-m708.png"
    sudo mkdir -p /usr/share/pixmaps
    sudo cp "${SCRIPT_DIR}/gui/assets/icon.png" /usr/share/pixmaps/ugee-m708.png
fi

mkdir -p "${HOME}/.local/share/applications"
cp "${SCRIPT_DIR}/packaging/ugee-gui.desktop" "${HOME}/.local/share/applications/ugee-gui.desktop"
chmod 0644 "${HOME}/.local/share/applications/ugee-gui.desktop"
sudo cp "${SCRIPT_DIR}/packaging/ugee-gui.desktop" /usr/share/applications/ugee-gui.desktop

gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
sudo gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database "${HOME}/.local/share/applications" 2>/dev/null || true
sudo update-desktop-database /usr/share/applications 2>/dev/null || true

# 5. Configura regras udev e libwacom
echo "[5/7] Configurando regras udev e definição libwacom..."
sudo rm -f /etc/udev/rules.d/99-ugee.rules /etc/udev/rules.d/99-ugee-m708.rules /etc/udev/rules.d/60-ugee-m708.rules
sudo cp "${SCRIPT_DIR}/udev/60-ugee.rules" /etc/udev/rules.d/60-ugee.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw --subsystem-match=usb --subsystem-match=misc || true

if ! id -nG "$USER" | grep -qw "input"; then
    echo "Adicionando o usuário '$USER' ao grupo 'input'..."
    sudo usermod -aG input "$USER"
fi

sudo mkdir -p /etc/libwacom
sudo cp "${SCRIPT_DIR}/packaging/ugee-m708.tablet" /etc/libwacom/ugee-m708.tablet

# 6. Configura e inicia o serviço systemd --user
echo "[6/7] Configurando serviço systemd --user (ugee-driver.service)..."
systemctl --user stop ugee-m708.service 2>/dev/null || true
systemctl --user disable ugee-m708.service 2>/dev/null || true

USER_SYSTEMD_DIR="${HOME}/.config/systemd/user"
mkdir -p "${USER_SYSTEMD_DIR}"
cp "${SCRIPT_DIR}/packaging/ugee-driver.service" "${USER_SYSTEMD_DIR}/ugee-driver.service"

systemctl --user daemon-reload
systemctl --user enable --now ugee-driver.service
systemctl --user restart ugee-driver.service

echo "[7/7] Verificando status do serviço..."
systemctl --user status ugee-driver.service --no-pager || true

echo ""
echo "=== Instalação concluída com sucesso! ==="
echo "• Interface gráfica: busque por 'Mesa Digitalizadora UGEE' no menu ou execute 'ugee-gui'."
echo "• Status do driver: ugee-driver --status  |  Inspeção de hardware: ugee-driver --probe"
echo "• Logs em tempo real: journalctl --user -u ugee-driver.service -f"
