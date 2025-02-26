#!/usr/bin/env sh
# Soar Package Manager Installation Script
# POSIX compliant installation script

set -eu

main() {
    DEFAULT_VERSION="latest"
    SOAR_VERSION="${SOAR_VERSION:-$DEFAULT_VERSION}"

    # ASCII Colors
    RED="\033[0;31m"
    GREEN="\033[0;32m"
    BLUE="\033[0;34m"
    YELLOW="\033[0;33m"
    RESET="\033[0m"

    # Function to check for a downloader, sorted by sanest choice
    check_download_tool() {
        if command -v curl >/dev/null 2>&1; then
            printf "curl -fSL -o"
            return 0
        elif command -v wget >/dev/null 2>&1; then
            printf "wget -O"
            return 0
        elif command -v soar-dl >/dev/null 2>&1; then
            printf "soar-dl -o"
            return 0
        elif command -v soar >/dev/null 2>&1; then
            printf "soar dl -o"
            return 0
        elif command -v aria2 >/dev/null 2>&1; then
            printf "aria2 -o"
            return 0
        elif command -v aria2c >/dev/null 2>&1; then
            printf "aria2c -o"
            return 0
        elif command -v axel >/dev/null 2>&1; then
            printf "axel -o"
            return 0
        elif command -v http >/dev/null 2>&1; then
            printf "http -Fdm -o"
            return 0
        elif command -v GET >/dev/null 2>&1; then
            printf "PERL_GET"
            return 0
        elif command -v python >/dev/null 2>&1; then
            printf "PYTHON_GET"
            return 0
        elif command -v python3 >/dev/null 2>&1; then
            printf "PYTHON3_GET"
            return 0
        elif command -v busybox >/dev/null 2>&1; then
            printf "busybox wget --no-check-certificate -O"
            return 0
        else
            printf "${RED}✗ Error: Could not find a downloader (curl, wget, soar-dl, aria2, axel, httpie, perl, python, busybox).${RESET}\n" >&2
            return 1
        fi
    }

    # Function to determine installation directory
    get_install_dir() {
        # Check environment variables first
        if [ -n "${SOAR_INSTALL_DIR-}" ]; then
            if [ -d "$SOAR_INSTALL_DIR" ] && [ -w "$SOAR_INSTALL_DIR" ]; then
                printf "%s" "$SOAR_INSTALL_DIR"
                return
            else
                printf "${RED}✗ Error: SOAR_INSTALL_DIR ${BLUE}($SOAR_INSTALL_DIR)${RED} is not writable or doesn't exist${RESET}\n" >&2
                exit 1
            fi
        fi

        if [ -n "${INSTALL_DIR-}" ]; then
            if [ -d "$INSTALL_DIR" ] && [ -w "$INSTALL_DIR" ]; then
                printf "%s" "$INSTALL_DIR"
                return
            else
                printf "${RED}✗ Error: INSTALL_DIR ${BLUE}($INSTALL_DIR)${RED} is not writable or doesn't exist${RESET}\n" >&2
                exit 1
            fi
        fi

        # Check ~/.local/bin
        local_bin="$HOME/.local/bin"
        if [ -d "$local_bin" ] && [ -w "$local_bin" ]; then
            printf "%s" "$local_bin"
            return
        fi

        # Fallback to /usr/local/bin if running as root
        if [ "$(id -u)" = "0" ]; then
            if [ -d "/usr/local/bin" ] && [ -w "/usr/local/bin" ]; then
                printf "/usr/local/bin"
                return
            fi
        fi

        # Fallback to current directory
        printf "${YELLOW}⚠ Notice: ${BLUE}~/.local/bin${YELLOW} not found or not writable. Installing in current directory.${RESET}\n" >&2
        printf "${YELLOW}You should move the binary to a location in your ${BLUE}\$PATH.${RESET}\n" >&2
        printf "%s" "$(pwd)"
    }

    # Function to download and install
    install_soar() {
        DOWNLOAD_TOOL=""
        if ! DOWNLOAD_TOOL=$(check_download_tool); then
           exit 1
        fi
        INSTALL_PATH=$(get_install_dir)

        # Detect architecture
        ARCH=$(uname -m)
        case "$ARCH" in
            x86_64)
                ARCH="x86_64"
                ;;
            aarch64)
                ARCH="aarch64"
                ;;
            *)
                printf "${RED}Error: Unsupported architecture: ${YELLOW}$ARCH${RESET}\n" >&2
                exit 1
                ;;
        esac

        # Get latest release URL
        printf "Downloading Soar..."
        case "$SOAR_VERSION" in
            *nightly*)
                RELEASE_URL="https://github.com/pkgforge/soar/releases/download/nightly/soar-$ARCH-linux"
                ;;
            *latest*)
                RELEASE_URL="https://github.com/pkgforge/soar/releases/latest/download/soar-$ARCH-linux"
                ;;
            *)
                RELEASE_URL="https://github.com/pkgforge/soar/releases/download/v$SOAR_VERSION/soar-$ARCH-linux"
                ;;
        esac           
        printf " <== $RELEASE_URL\n"

        # Download and install
        if [ "$DOWNLOAD_TOOL" = "PERL_GET" ]; then
           printf "[+] Using GET\n"
           GET "$RELEASE_URL" > "$INSTALL_PATH/soar"
           printf "\n"
        elif [ "$DOWNLOAD_TOOL" = "PYTHON_GET" ]; then
           printf "[+] Using python -c\n"
           python -c "import urllib.request; urllib.request.urlretrieve('$RELEASE_URL', '$INSTALL_PATH/soar')"
           printf "\n"
        elif [ "$DOWNLOAD_TOOL" = "PYTHON3_GET" ]; then
           printf "[+] Using python3 -c\n"
           python3 -c "import urllib.request; urllib.request.urlretrieve('$RELEASE_URL', '$INSTALL_PATH/soar')"
           printf "\n"
        else
           printf "[+] Using $DOWNLOAD_TOOL\n"
           $DOWNLOAD_TOOL "$INSTALL_PATH/soar" "$RELEASE_URL"
        fi
        if [ ! -f "$INSTALL_PATH/soar" ]; then
            printf "${RED}Error: Download failed${RESET}\n"
            exit 1
        fi
        # Make executable
        chmod +x "$INSTALL_PATH/soar"
        "$INSTALL_PATH/soar" --version
        printf "${GREEN}✓ Soar has been installed to: ${BLUE}$INSTALL_PATH/soar${RESET}\n"
        printf "${YELLOW}ⓘ Make sure ${BLUE}$INSTALL_PATH${YELLOW} is in your ${BLUE}PATH.${RESET}\n"
        printf "${YELLOW}ⓘ Documentation: ${BLUE}https://soar.qaidvoid.dev${RESET}\n"
        printf "${YELLOW}ⓘ Discord: ${BLUE}https://docs.pkgforge.dev/contact/chat${RESET}\n"
        printf "${YELLOW}ⓘ External Repositories are ${RED}NOT Enabled${YELLOW} by default${RESET}\n"
        printf "${YELLOW}ⓘ Learn More: ${BLUE}https://docs.pkgforge.dev/repositories/external${RESET}\n"
        printf "${YELLOW}ⓘ To enable external repos, Run: ${GREEN}soar defconfig --external${RESET}\n"
    }

    # Execute installation
    install_soar
}

# Call main function
main
