##Bash completion for `soar`
#Ideal Locations: $DATA_DIR/bash-completion/completions/soar

#Shared Sanity Check logic
_soar_sanity_check() {
  hash -r &>/dev/null
  #Check required commands
  for cmd in awk cut date getent grep jq mapfile soar sort stat tr uname; do
    command -v "${cmd}" >/dev/null || return 1
  done
  #Ensure USER
  if [[ -z "${USER+x}" || -z "${USER##*[[:space:]]}" ]]; then
    USER="$(whoami | tr -d '[:space:]')"
  fi
  #Ensure HOME
  if [[ -z "${HOME+x}" || -z "${HOME##*[[:space:]]}" ]]; then
    HOME="$(getent passwd "${USER}" 2>/dev/null | awk -F: '{print $6}' | tr -d '[:space:]')"
  fi
  #If has XDG
  if [[ -n "${XDG_CACHE_HOME}" ]]; then
     cache_dir="${XDG_CACHE_HOME}/soar"
  else
     cache_dir="${HOME}/.cache/soar"
  fi
  #Ensure cache_dir is writable
  [[ -d "${cache_dir}" ]] || mkdir -p "${cache_dir}" || return 1
  [[ -w "${cache_dir}" ]] || return 1
  return 0
}

#Completion for `soar add` with fzf support
_soar_add_completions() {
  local cur="${COMP_WORDS[COMP_CWORD]}"
  local cache="${cache_dir}/soar-add-autocomplete.txt"
  local cache_src
  cache_src="https://raw.githubusercontent.com/pkgforge/metadata/refs/heads/main/completions/ALL_$(uname -m)-Linux.txt"
  _soar_sanity_check || return

  local regenerate=0
  if [[ ! -f "${cache}" ]]; then
    regenerate=1
  else
    local file_mtime
    local now
    file_mtime="$(stat -c '%Y' "${cache}")"
    now="$(date +%s)"
    #6 HRs
    if (( now - file_mtime > 36000 )); then
      regenerate=1
    fi
  fi

  if (( regenerate )); then
    soar dl "${cache_src}" --output "${cache}" --quiet --no-color &>/dev/null
  fi

  if [[ ${COMP_WORDS[1]} == "add" ||\
        ${COMP_WORDS[1]} == "dl" ||\
        ${COMP_WORDS[1]} == "download" ||\
        ${COMP_WORDS[1]} == "exec" ||\
        ${COMP_WORDS[1]} == "execute" ||\
        ${COMP_WORDS[1]} == "i" ||\
        ${COMP_WORDS[1]} == "install" ||\
        ${COMP_WORDS[1]} == "inspect" ||\
        ${COMP_WORDS[1]} == "log" ||\
        ${COMP_WORDS[1]} == "Q" ||\
        ${COMP_WORDS[1]} == "query" ||\
        ${COMP_WORDS[1]} == "run" ]] &&\
        [[ -f "${cache}" ]]; then
    #If fzf
    if command -v fzf >/dev/null; then
      local selected
      ##These still don't support full pkg_name#pkg_id:pkg_repo format
      if [[  ${COMP_WORDS[1]} == "dl" ||\
             ${COMP_WORDS[1]} == "download" ||\
             ${COMP_WORDS[1]} == "exec" ||\
             ${COMP_WORDS[1]} == "execute" ||\
             ${COMP_WORDS[1]} == "log" ||\
             ${COMP_WORDS[1]} == "run" ]]; then
         selected="$(cat "${cache}" | fzf --exit-0 --query="${cur}" --cycle --exact \
           --highlight-line --ignore-case --no-sort --reverse --select-1 --wrap | awk -F '#' '{print $1}' | sort -u)"
      else
         selected="$(cat "${cache}" | fzf --exit-0 --query="${cur}" --cycle --exact \
           --highlight-line --ignore-case --no-sort --reverse --select-1 --wrap | awk -F ' ## ' '{print $1}')"
      fi
      if [[ -n "${selected}" ]]; then
        COMPREPLY=("${selected}")
        compopt -o nospace
        return 0
      fi
    fi
    
    #If no fzf
    local pkg_ids
    pkg_ids="$(awk -F ' ## ' '{print $1}' "${cache}")"
    mapfile -t "COMPREPLY" < <(compgen -W "${pkg_ids}" -- "${cur}")    
    #Show description if ? is used
    if [[ "${cur}" == *"?" ]]; then
      cur="${cur%?}"
      printf '\n'
      grep -E "^${cur}" "${cache}"
      COMPREPLY=("${cur}")
      compopt -o nospace
    fi

    #If low matches, then show descriptions
    if [[ ${#COMPREPLY[@]} -gt 0 && ${#COMPREPLY[@]} -lt 10 ]]; then
      printf '\n'
      for match in "${COMPREPLY[@]}"; do
        grep -E "^${match}#" "${cache}" || echo "${match}"
      done
      compopt -o nospace
    fi
  fi
}

#Completion for `soar remove` with fzf support
_soar_remove_completions() {
  local cur="${COMP_WORDS[COMP_CWORD]}"
  local cache="${cache_dir}/soar-remove-autocomplete.txt"

  _soar_sanity_check || return

  #local regenerate=0
  #if [[ ! -f "${cache}" ]]; then
  #  regenerate=1
  #else
  #  local file_mtime
  #  local now
  #  file_mtime="$(stat -c '%Y' "${cache}")"
  #  now="$(date +%s)"
  #  #6 HRs
  #  if (( now - file_mtime > 36000 )); then
  #    regenerate=1
  #  fi
  #fi

  #if (( regenerate )); then
    soar info --json --no-color 2>/dev/null | \
      jq -r '.. | objects | select(.pkg_name? and .repo_name?)
      | "\(.pkg_name):\(.repo_name)"' | sort -u -o "${cache}"
  #fi

  if [[ ${COMP_WORDS[1]} == "del" || ${COMP_WORDS[1]} == "r" || ${COMP_WORDS[1]} == "remove" ]] && [[ -f "${cache}" ]]; then
    #If fzf
    if command -v fzf >/dev/null && [[ -z "${cur}" ]]; then
      local selected
      selected="$(cat "${cache}" | fzf --exit-0 \
        --cycle --exact --highlight-line --ignore-case --no-sort --reverse --select-1 --wrap)"
      if [[ -n "${selected}" ]]; then
        COMPREPLY=("${selected}")
        compopt -o nospace
        return 0
      fi
    fi
    #If no fzf
    mapfile -t lines < "${cache}"
    mapfile -t "COMPREPLY" < <(compgen -W "${lines[*]}" -- "${cur}")
    compopt -o nospace
  fi
}

#Completion for base `soar` command with descriptions and fzf support
_soar_base_completions() {
  local cur="${COMP_WORDS[COMP_CWORD]}"
  
  #Only provide completions when we're at position 1 (the subcommand)
  if [[ ${COMP_CWORD} -eq 1 ]]; then

    #Define commands and their descriptions
    local commands=(
      "add                    - Install packages [alias for install]"
      "clean                  - Garbage collection"
      "config                 - Print the configuration file to stdout"
      "del                    - Remove packages [alias for remove]"
      "defconfig --external   - Generate default config with external repos enabled"
      "download               - Download arbitrary files [aliases: dl]"
      "env                    - View currently configured environment variables"
      "find                   - Search package [alias for search]"
      "health                 - Health check"
      "help                   - Print this message or the help of the given subcommand(s)"
      "info                   - Show info about installed packages [aliases: list-installed]"
      "inspect                - Inspect package build script"
      "install                - Install packages [aliases: i, add]"
      "list                   - List all available packages [aliases: ls]"
      "log                    - Inspect package build log"
      "search                 - Search package [aliases: s, find]"
      "query                  - Query package info [aliases: Q]"
      "remove                 - Remove packages [aliases: r, del]"
      "run                    - Run packages without installing to PATH [aliases: exec, execute]"
      "update                 - Update packages [aliases: u, upgrade]"
      "use                    - Use package from different family"
      "self                   - Modify the soar installation"
      "sync                   - Sync with remote metadata [aliases: S, fetch]"
    )

   #If fzf
     if command -v fzf >/dev/null && [[ -z "${cur}" ]]; then
       local cmd_list=""
       for cmd in "${commands[@]}"; do
         cmd_list+="${cmd}\n"
       done
       local selected
       selected="$(echo -e "${cmd_list}" | fzf --exit-0 --cycle --exact \
         --highlight-line --ignore-case --reverse --select-1 --wrap | awk -F ' - ' '{gsub(/^ +| +$/, "", $1); print $1}')"
       if [[ -n "${selected}" ]]; then
         COMPREPLY=("${selected}")
         return 0
       fi
     fi

   #If no fzf
     if ! command -v fzf >/dev/null; then
       if [[ "${cur}" == "" ]]; then
         printf '\n'
         printf '%s\n' "${commands[@]}"
         COMPREPLY=("")
       else
         local filtered_commands=()
         for cmd in "${commands[@]}"; do
           if [[ ${cmd} == "${cur}"* ]]; then
             filtered_commands+=("${cmd}")
           fi
         done
   
         if [[ ${#filtered_commands[@]} -eq 1 ]]; then
           COMPREPLY=("${filtered_commands[0]%% *}")
         elif [[ ${#filtered_commands[@]} -gt 1 ]]; then
           printf '\n'
           printf '%s\n' "${filtered_commands[@]}"
           COMPREPLY=("${cur}")
         fi
       fi
       compopt -o nospace
       return 0
     fi
  else

   #If subcommand already present
    case "${COMP_WORDS[1]}" in
      add|dl|download|exec|execute|i|install|inspect|log|Q|query|run)
        _soar_add_completions
        ;;
      remove|r|del)
        _soar_remove_completions
        ;;
      *)
        #Placeholder for other subcommands
        ;;
    esac
  fi
}

#Register completions
complete -F _soar_base_completions soar
