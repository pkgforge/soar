#!/usr/bin/env sh
# Soar Installation Script (v1.0.0)
# Tries to be as POSIX compliant as possible (Any deviation is intentional)
# Assumptions: User has a supported downloader
# Supported Downloaders:
#  aria2 (aria2c) axel bash (/dev/tcp) busybox curl http (httpie) nushell (http) perl (libww) python/python3 soar-dl soar wget
# If no supported downloaders are found (happens often), a fallback to bash+sed or nushell is used

set -eu
#shellcheck disable=SC2016,SC2059
main() {

  # Enable Debug?
  DEBUG="${DEBUG:-}"
  if [ -n "$DEBUG" ]; then
    set -x
  fi

  # Default
  DEFAULT_VERSION="latest"
  SOAR_VERSION="${SOAR_VERSION:-$DEFAULT_VERSION}"

  # ASCII Colors
  RED="\033[0;31m"
  GREEN="\033[0;32m"
  BLUE="\033[0;34m"
  YELLOW="\033[0;33m"
  RESET="\033[0m"

  # Refresh command -v
  if command -v hash >/dev/null 2>&1; then
    hash -r >/dev/null 2>&1
  fi

  # Ensure HOME
  if [ -z "$HOME" ]; then
    printf "${YELLOW}⚠ Notice: ${BLUE}\$HOME${YELLOW} is not set. (Will try guessing...)${RESET}\n" >&2

    case $(
      command -v awk getent grep tr whoami >/dev/null 2>&1
      echo $?
    ) in
    0)
      [ -z "$USER" ] && {
        printf "${YELLOW}⚠ Notice: ${BLUE}\$USER${YELLOW} is not set. (Will try guessing...)${RESET}\n" >&2
        USER="$(whoami | tr -d '[:space:]')"
      }
      HOME="$(getent passwd "$USER" | awk -F':' 'NF >= 6 {print $6}' | tr -d '[:space:]')"
      ;;
    esac

    if [ ! -d "$HOME" ]; then
      printf "${RED}✗ Error: Could not determine ${BLUE}\$HOME${RESET}\n" >&2
    else
      printf "${YELLOW}⚠ Notice: ${BLUE}\$HOME${YELLOW} is set to ${BLUE}$HOME${RESET}\n" >&2
    fi
  fi

  # Check XDG/Fallback Vars & Perms
  for var in X_BIN X_CONFIG X_CACHE X_DATA; do
    dir_var=""
    dir_path=""

    case $var in
    X_BIN)
      dir_var="BIN_DIR"
      xdg_var="XDG_BIN_HOME"
      fallback="$HOME/.local/bin"
      ;;
    X_CONFIG)
      dir_var="CONFIG_DIR"
      xdg_var="XDG_CONFIG_HOME"
      fallback="$HOME/.config"
      ;;
    X_CACHE)
      dir_var="CACHE_DIR"
      xdg_var="XDG_CACHE_HOME"
      fallback="$HOME/.cache"
      ;;
    X_DATA)
      dir_var="DATA_DIR"
      xdg_var="XDG_DATA_HOME"
      fallback="$HOME/.local/share"
      ;;
    esac

    xdg_value=""
    eval "xdg_value=\"\${$xdg_var:-}\""

    if [ -n "$xdg_value" ]; then
      xdg_path="$xdg_value"
      mkdir -p "$xdg_path" >/dev/null 2>&1
      if [ -d "$xdg_path" ] && [ -w "$xdg_path" ]; then
        eval "$dir_var=\"$xdg_path\""
        continue
      fi
    fi

    if [ -n "$HOME" ]; then
      mkdir -p "$fallback" >/dev/null 2>&1
      if [ -d "$fallback" ] && [ -w "$fallback" ]; then
        eval "$dir_var=\"$fallback\""
      fi
    fi
  done

  # Check for a downloader, sorted by sanest choice
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
    elif command -v nu >/dev/null 2>&1; then
      printf "NU_HTTP"
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
    elif command -v bash >/dev/null 2>&1 && command -v sed >/dev/null 2>&1; then
      printf "BASH_DEV_TCP"
      return 0
    else
      printf "${RED}✗ Error: Could not find a downloader (curl, wget, soar-dl, aria2, axel, httpie, perl, python, busybox).${RESET}\n" >&2
      return 1
    fi
  }

  # Determine installation directory
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

    # Check Writable BIN Dir
    if [ -n "$BIN_DIR" ]; then
      if [ -d "$BIN_DIR" ] && [ -w "$BIN_DIR" ]; then
        printf "%s" "$BIN_DIR"
        return
      fi
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
    aarch64)
      ARCH="aarch64"
      ;;
    loongarch64)
      ARCH="loongarch64"
      ;;
    riscv64)
      ARCH="riscv64"
      ;;
    x86_64)
      ARCH="x86_64"
      ;;
    *)
      printf "${RED}Error: Unsupported architecture: ${YELLOW}$ARCH${RESET}\n" >&2
      exit 1
      ;;
    esac

    # Detect OS
    OS=$(uname -s)
    case "$OS" in
    Linux)
      OS="linux"
      ;;
    *)
      printf "${RED}Error: Unsupported operating system: ${YELLOW}$OS${RESET}\n" >&2
      printf "${RED}Only Linux is currently supported${RESET}\n" >&2
      exit 1
      ;;
    esac

    # Get latest release URL
    printf "Downloading Soar..."
    case "$SOAR_VERSION" in
    *nightly*)
      RELEASE_URL="https://github.com/pkgforge/soar/releases/download/nightly/soar-$ARCH-$OS"
      ;;
    *latest*)
      RELEASE_URL="https://github.com/pkgforge/soar/releases/latest/download/soar-$ARCH-$OS"
      ;;
    *)
      RELEASE_URL="https://github.com/pkgforge/soar/releases/download/v$SOAR_VERSION/soar-$ARCH-$OS"
      ;;
    esac
    printf " <== $RELEASE_URL\n"

    # Download and install
    if [ "$DOWNLOAD_TOOL" = "NU_HTTP" ]; then
      printf "[+] Using HTTP (nushell)\n"
      RELEASE_URL="http://http.pkgforge.dev/$RELEASE_URL" INSTALL_PATH="$INSTALL_PATH" nu --no-config-file -c \
        'http get --redirect-mode follow --insecure --raw $env.RELEASE_URL | save -f ($env.INSTALL_PATH + "/soar")'
      printf "\n"
    elif [ "$DOWNLOAD_TOOL" = "PERL_GET" ]; then
      printf "[+] Using GET\n"
      GET "$RELEASE_URL" >"$INSTALL_PATH/soar"
      printf "\n"
    elif [ "$DOWNLOAD_TOOL" = "PYTHON_GET" ]; then
      printf "[+] Using python -c\n"
      python -c "import urllib.request; urllib.request.urlretrieve('$RELEASE_URL', '$INSTALL_PATH/soar')"
      printf "\n"
    elif [ "$DOWNLOAD_TOOL" = "PYTHON3_GET" ]; then
      printf "[+] Using python3 -c\n"
      python3 -c "import urllib.request; urllib.request.urlretrieve('$RELEASE_URL', '$INSTALL_PATH/soar')"
      printf "\n"
    elif [ "$DOWNLOAD_TOOL" = "BASH_DEV_TCP" ]; then
      printf "\n${YELLOW}⚠ Attempting to download using ${BLUE}Bash${YELLOW} (${GREEN}/dev/tcp${YELLOW}) over HTTP${RESET}\n" >&2
      printf "${YELLOW}⚠ This is highly unreliable & may not Work${RESET}\n\n" >&2
      RELEASE_URL="http://http.pkgforge.dev/$RELEASE_URL" INSTALL_PATH="$INSTALL_PATH" bash -c \
        '
           raw_http_get() {
           #Get Input
            url=$1
            port=${2:-80}
           #Actually Verify we are in bash
            is_bash=0
            [[ -n "${BASH}" ]] && is_bash=1
            if [[ $is_bash -eq 0 ]]; then
              (shopt -p >/dev/null 2>&1) && is_bash=1
            fi
           #Proceed
            if [ $is_bash -eq 1 ]; then
             #Parse Input
              url=${url#http://}
              url=${url#https://}
              host=${url%%/*}
              if [[ "$url" = "$host" ]]; then
                path="/"
              else
                path="/${url#$host/}"
              fi
             #Download
              exec 3<>/dev/tcp/$host/$port
              echo -e "GET $path HTTP/1.1\r\nHost: $host\r\nConnection: close\r\n\r\n" >&3
              if command -v dd >/dev/null 2>&1; then
                 dd bs=1K <&3
              elif command -v cat >/dev/null 2>&1; then    
                 cat <&3
              fi
              exec 3>&-
            else
              echo "Error: No method available to make HTTP requests. Requires Bash with /dev/tcp" >&2
              return 1
            fi
           }
           raw_http_get "${RELEASE_URL}" > "${INSTALL_PATH}/soar"
           if [[ -s "${INSTALL_PATH}/soar" ]]; then
             #Removes HTTP Headers
             sed "1,/^\r\{0,1\}$/d" -i "${INSTALL_PATH}/soar"
           fi
          '
      printf "\n"
    else
      printf "[+] Using $DOWNLOAD_TOOL\n"
      $DOWNLOAD_TOOL "$INSTALL_PATH/soar" "$RELEASE_URL"
    fi
    # Check
    if [ ! -f "$INSTALL_PATH/soar" ]; then
      if [ "$DOWNLOAD_TOOL" = "BASH_DEV_TCP" ]; then
        printf "${RED}Error: Download failed.${YELLOW} Install ${BLUE}curl/wget${YELLOW} & try again${RESET}\n"
      else
        printf "${RED}Error: Download failed${RESET}\n"
      fi
      exit 1
    fi
    # Make executable
    chmod +x "$INSTALL_PATH/soar"
    # Check for valid elf in case sed failed
    if [ "$DOWNLOAD_TOOL" = "BASH_DEV_TCP" ]; then
      if ! "$INSTALL_PATH/soar" --version >/dev/null 2>&1; then
        printf "${RED}Error: Failed to properly extract soar.${YELLOW} Install ${BLUE}curl/wget${YELLOW} & try again${RESET}\n"
        rm -fv "$INSTALL_PATH/soar"
        printf "\n"
        exit 1
      fi
    fi
    # Check & Print Docs
    "$INSTALL_PATH/soar" --version || printf "${RED}Error: Failed to properly download soar${RESET}"
    printf "\n${GREEN}✓ Soar has been installed to: ${BLUE}$INSTALL_PATH/soar${RESET}\n"
    printf "${YELLOW}ⓘ Documentation: ${BLUE}https://soar.qaidvoid.dev${RESET}\n"
    printf "${YELLOW}ⓘ Discord: ${BLUE}https://docs.pkgforge.dev/contact/chat${RESET}\n"
    # Check if in PATH
    if command -v expr >/dev/null 2>&1; then
      if expr ":$PATH:" : ".*:$BIN_DIR:" >/dev/null ||
        expr ":$PATH:" : ".*:$(expr "$BIN_DIR" : '\(.*\)/$'):" >/dev/null; then
        :
      else
        printf "\n${YELLOW}⚠ ${BLUE}$INSTALL_PATH${RED} is NOT in your ${BLUE}PATH${RESET}\n"
        printf "${YELLOW}ⓘ Put this in your ${BLUE}SHELL/Profile${YELLOW}:${RESET}\n"
        printf "\n${GREEN} export PATH=\"\$PATH:$INSTALL_PATH\"${RESET}\n\n"
      fi
    else
      printf "${YELLOW}ⓘ Make sure ${BLUE}$INSTALL_PATH${YELLOW} is in your ${BLUE}PATH.${RESET}\n"
    fi
    # Enable External Repos
    printf "${YELLOW}ⓘ External Repositories are ${RED}NOT Enabled${YELLOW} by default${RESET}\n"
    printf "${YELLOW}ⓘ Learn More: ${BLUE}https://docs.pkgforge.dev/repositories/external${RESET}\n"
    printf "${YELLOW}ⓘ To enable external repos, Run: ${GREEN}soar defconfig --external${RESET}\n"
    # Sync
    printf "${YELLOW}ⓘ Finally, To synchronize all repos, Run: ${GREEN}soar sync${RESET}\n"
    # Check Current Config
    SOAR_ENV_OUT="$($INSTALL_PATH/soar env 2>/dev/null)"
    if [ -n "$SOAR_ENV_OUT" ]; then
      if command -v awk >/dev/null 2>&1 && command -v expr >/dev/null 2>&1; then
        SOAR_BIN_PATH="$(printf "$SOAR_ENV_OUT" | awk -F= '/^SOAR_BIN=/{print $2}')"
        if [ -n "$SOAR_BIN_PATH" ]; then
          if expr ":$PATH:" : ".*:$SOAR_BIN_PATH:" >/dev/null ||
            expr ":$PATH:" : ".*:$(expr "$SOAR_BIN_PATH" : '\(.*\)/$'):" >/dev/null; then
            :
          else
            printf "\n${YELLOW}⚠ ${BLUE}$SOAR_BIN_PATH${RED} is NOT in your ${BLUE}\$PATH${RESET}\n"
            printf "${YELLOW}ⓘ Put this in your ${BLUE}SHELL/Profile${YELLOW}:${RESET}\n"
            printf "\n${GREEN} export PATH=\"\$PATH:$INSTALL_PATH:$SOAR_BIN_PATH\"${RESET}\n\n"
          fi
        fi
      fi
    fi
    # Print Current config
    printf "\n${YELLOW}ⓘ Current Soar Configuration:${RESET}\n\n"
    "$INSTALL_PATH/soar" env
    printf "\n"
  }

  # Run Installation
  install_soar

  # Disable Debug?
  if [ -z "$DEBUG" ]; then
    :
  elif [ -n "$DEBUG" ]; then
    set +x
  fi
}

# Call main function
main
