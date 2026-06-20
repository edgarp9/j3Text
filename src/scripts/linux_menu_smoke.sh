#!/usr/bin/env bash
set -euo pipefail

if ! command -v xdotool >/dev/null 2>&1; then
  echo "SKIP: xdotool is required for Linux menu smoke" >&2
  exit 77
fi

tmp_config=$(mktemp -d)
exit_config=$(mktemp -d)
action_config=$(mktemp -d)
log_file=$(mktemp)
exit_log_file=$(mktemp)
action_log_file=$(mktemp)
action_report_file=$(mktemp)
action_new_window_path_file=$(mktemp)
restore_log_file=$(mktemp)
restore_report_file=$(mktemp)
smoke_file="${tmp_config}/smoke.txt"
action_open_file="${tmp_config}/action-open.txt"
action_save_as_file="${tmp_config}/action-save-as.txt"
ui_exe="${tmp_config}/bin/j3text"
exit_exe="${exit_config}/bin/j3text"
action_exe="${action_config}/bin/j3text"
settings_file="${action_exe}.toml"
last_click=""
pid=""

printf 'seed\n' >"${smoke_file}"
printf 'action open\n' >"${action_open_file}"

cleanup() {
  stop_app
  rm -rf "${tmp_config}" "${exit_config}" "${action_config}" "${log_file}" "${exit_log_file}" "${action_log_file}" "${action_report_file}" "${action_new_window_path_file}" "${restore_log_file}" "${restore_report_file}"
}
trap cleanup EXIT

copy_smoke_exe() {
  local target=$1
  mkdir -p "$(dirname "${target}")"
  cp target/debug/j3text "${target}"
  chmod +x "${target}"
}

stop_app() {
  if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
  fi
  pid=""
}

fail_with_log() {
  echo "FAIL: $1" >&2
  if [ -n "${window_id:-}" ]; then
    echo "window id: ${window_id}" >&2
    echo "last click: ${last_click}" >&2
    if [ -n "${pid}" ]; then
      ps -o pid,stat,comm,args -p "${pid}" >&2 || true
    fi
    local screenshot="/tmp/j3text-linux-smoke-fail.png"
    if xwd -silent -id "${window_id}" -out "${screenshot}.xwd" 2>/dev/null && convert "${screenshot}.xwd" "${screenshot}" 2>/dev/null; then
      echo "failure screenshot: ${screenshot}" >&2
      rm -f "${screenshot}.xwd"
    fi
  fi
  if [ -f "${smoke_file}" ]; then
    echo "smoke file content:" >&2
    sed -n '1,20p' "${smoke_file}" >&2
  fi
  if [ -s "${action_report_file}" ]; then
    echo "action smoke report:" >&2
    sed -n '1,120p' "${action_report_file}" >&2
  fi
  if [ -s "${restore_report_file}" ]; then
    echo "restore smoke report:" >&2
    sed -n '1,120p' "${restore_report_file}" >&2
  fi
  sed -n '1,120p' "${log_file}" >&2
  if [ -s "${exit_log_file}" ]; then
    echo "exit smoke log:" >&2
    sed -n '1,160p' "${exit_log_file}" >&2
  fi
  if [ -s "${action_log_file}" ]; then
    echo "action smoke log:" >&2
    sed -n '1,160p' "${action_log_file}" >&2
  fi
  if [ -s "${restore_log_file}" ]; then
    echo "restore smoke log:" >&2
    sed -n '1,160p' "${restore_log_file}" >&2
  fi
  exit 1
}

ensure_running() {
  if ! kill -0 "${pid}" 2>/dev/null; then
    fail_with_log "j3Text exited during ${1}"
  fi
}

find_j3text_window() {
  for args in \
    '--onlyvisible --name j3Text' \
    '--onlyvisible --class j3text' \
    '--name j3Text' \
    '--class j3text'
  do
    # shellcheck disable=SC2086
    xdotool search ${args} 2>/dev/null | awk '/^[0-9]+$/ { print }'
  done | while read -r candidate; do
    local candidate_pid
    candidate_pid=$(xdotool getwindowpid "${candidate}" 2>/dev/null | awk '/^[0-9]+$/ { print; exit }' || true)
    [ "${candidate_pid}" = "${pid}" ] || continue
    local geometry
    local X=0
    local Y=0
    local WIDTH=0
    local HEIGHT=0
    geometry=$(xdotool getwindowgeometry --shell "${candidate}" 2>/dev/null | awk -F= '/^(X|Y|WIDTH|HEIGHT)=/ { print }')
    eval "${geometry}"
    if [ "${WIDTH:-0}" -ge 400 ] && [ "${HEIGHT:-0}" -ge 300 ]; then
      echo "${candidate}"
      return 0
    fi
  done
}

press_key() {
  local label=$1
  shift
  if ! xdotool key --clearmodifiers "$@" >/dev/null 2>&1; then
    fail_with_log "xdotool key failed during ${label}"
  fi
  sleep 0.1
  ensure_running "${label}"
}

type_text() {
  local label=$1
  local text=$2
  if ! xdotool type --delay 50 --clearmodifiers "${text}" >/dev/null 2>&1; then
    fail_with_log "xdotool type failed during ${label}"
  fi
  sleep 0.4
  ensure_running "${label}"
}

type_smoke_marker() {
  local label=$1
  type_text "${label}: linux" "linux"
  type_text "${label}: smoke" " smoke"
  type_text "${label}: edit" " edit"
}

click_editor() {
  local label=$1
  if ! xdotool windowactivate --sync "${window_id}" >/dev/null 2>&1; then
    fail_with_log "failed to activate j3Text window during ${label}"
  fi
  sleep 0.1
  last_click="window:${window_id}:220,180"
  if ! xdotool mousemove --window "${window_id}" 220 180 click 1 >/dev/null 2>&1; then
    fail_with_log "xdotool click failed during ${label}"
  fi
  sleep 0.2
  ensure_running "${label}"
}

right_click_window_point() {
  local label=$1
  local x_offset=$2
  local y_offset=$3
  if ! xdotool windowactivate --sync "${window_id}" >/dev/null 2>&1; then
    fail_with_log "failed to activate j3Text window during ${label}"
  fi
  sleep 0.1
  last_click="window:${window_id}:${x_offset},${y_offset}"
  if ! xdotool mousemove --window "${window_id}" "${x_offset}" "${y_offset}" click 3 >/dev/null 2>&1; then
    fail_with_log "xdotool right click failed during ${label}"
  fi
  sleep 0.2
  ensure_running "${label}"
  press_key "${label}: close context menu" Escape
}

right_click_editor_context() {
  local label=$1
  if ! xdotool windowactivate --sync "${window_id}" >/dev/null 2>&1; then
    fail_with_log "failed to activate j3Text window during ${label}"
  fi
  sleep 0.1
  last_click="window:${window_id}:220,180"
  if ! xdotool mousemove --window "${window_id}" 220 180 click 3 >/dev/null 2>&1; then
    fail_with_log "xdotool editor right click failed during ${label}"
  fi
  sleep 0.2
  ensure_running "${label}"
  press_key "${label}: close context menu" Escape
}

activate_menu_item() {
  local label=$1
  local top_index=$2
  local item_index=$3

  press_key "${label}: focus menubar" F10
  press_key "${label}: reset menubar focus" Home
  for _ in $(seq 1 "${top_index}"); do
    press_key "${label}: choose top-level menu" Right
  done
  for _ in $(seq 1 "${item_index}"); do
    press_key "${label}: choose menu item" Down
  done
  press_key "${label}: activate menu item" Return
}

click_file_menu_item() {
  local label=$1
  local y_offset=$2

  if ! xdotool windowactivate --sync "${window_id}" >/dev/null 2>&1; then
    fail_with_log "failed to activate j3Text window during ${label}"
  fi
  sleep 0.1
  last_click="window:${window_id}:30,22"
  if ! xdotool mousemove --window "${window_id}" 30 22 click 1 >/dev/null 2>&1; then
    fail_with_log "xdotool click failed during ${label}: open File menu"
  fi
  sleep 0.2
  last_click="window:${window_id}:32,${y_offset}"
  if ! xdotool mousemove --window "${window_id}" 32 "${y_offset}" click 1 >/dev/null 2>&1; then
    fail_with_log "xdotool click failed during ${label}: activate File menu item"
  fi
  sleep 0.4
}

activate_exit_menu_item() {
  local label=$1
  local _item_index=${2:-}
  local geometry
  local X=0
  local Y=0

  click_file_menu_item "${label}: click Exit" 358
  sleep 0.2
  for _ in $(seq 1 50); do
    if ! kill -0 "${pid}" 2>/dev/null; then
      if wait "${pid}"; then
        pid=""
        return
      fi
      pid=""
      fail_with_log "${label} exited with failure"
    fi
    sleep 0.1
  done
  fail_with_log "${label} did not exit the app"
}

activate_submenu_item() {
  local label=$1
  local top_index=$2
  local item_index=$3
  local submenu_index=$4

  press_key "${label}: focus menubar" F10
  press_key "${label}: reset menubar focus" Home
  for _ in $(seq 1 "${top_index}"); do
    press_key "${label}: choose top-level menu" Right
  done
  for _ in $(seq 1 "${item_index}"); do
    press_key "${label}: choose submenu parent" Down
  done
  press_key "${label}: open submenu" Right
  for _ in $(seq 1 "${submenu_index}"); do
    press_key "${label}: choose submenu item" Down
  done
  press_key "${label}: activate submenu item" Return
}

cancel_modal() {
  local label=$1
  sleep 0.3
  press_key "${label}: cancel modal" Escape
  sleep 0.2
  ensure_running "${label}"
}

assert_no_severe_log() {
  local label=$1
  local file=$2
  if [ ! -s "${file}" ]; then
    return
  fi
  if grep -E "thread '.*' panicked|panicked at|RefCell already borrowed|This application can not open files|Gtk-ERROR|Gdk-ERROR|GLib-ERROR|failed to write Linux action smoke report" "${file}" >&2; then
    fail_with_log "${label} emitted a severe runtime log"
  fi
}

resize_window() {
  local label=$1
  local width=$2
  local height=$3
  if ! xdotool windowsize "${window_id}" "${width}" "${height}" >/dev/null 2>&1; then
    fail_with_log "xdotool windowsize failed during ${label}"
  fi
  sleep 0.3
  ensure_running "${label}"
  if ! xdotool windowactivate --sync "${window_id}" >/dev/null 2>&1; then
    fail_with_log "failed to reactivate j3Text window during ${label}"
  fi
  sleep 0.2
}

cargo build --quiet >"${log_file}" 2>&1 || fail_with_log "j3Text debug build failed"
copy_smoke_exe "${ui_exe}"
copy_smoke_exe "${exit_exe}"
copy_smoke_exe "${action_exe}"

GDK_BACKEND="${J3TEXT_SMOKE_GDK_BACKEND:-x11}" XDG_CONFIG_HOME="${tmp_config}" "${ui_exe}" "${smoke_file}" >"${log_file}" 2>&1 &
pid=$!

window_id=""
for _ in $(seq 1 200); do
  window_id=$(find_j3text_window || true)
  if [ -n "${window_id}" ]; then
    break
  fi
  if ! kill -0 "${pid}" 2>/dev/null; then
    fail_with_log "j3Text exited before a window appeared"
  fi
  sleep 0.1
done

if [ -z "${window_id}" ]; then
  fail_with_log "j3Text window was not found"
fi

if ! xdotool windowactivate --sync "${window_id}" >/dev/null 2>&1; then
  fail_with_log "failed to activate j3Text window"
fi
sleep 1.5

resize_window "compact window layout smoke" 640 480
resize_window "default window layout smoke" 800 600

for attempt in 1 2 3; do
  click_editor "focus editor for file edit"
  press_key "select all before file edit attempt ${attempt}" ctrl+a
  type_text "restore seed before file edit attempt ${attempt}" "seed"
  press_key "restore newline before file edit attempt ${attempt}" Return
  type_smoke_marker "edit file-backed document attempt ${attempt}"
  click_editor "refocus editor before save attempt ${attempt}"
  click_file_menu_item "File/Save menu item after file edit" 188
  ensure_running "File/Save menu item after file edit"
  for _ in $(seq 1 20); do
    if grep -q "linux smoke edit" "${smoke_file}"; then
      break
    fi
    sleep 0.1
  done
  if grep -q "linux smoke edit" "${smoke_file}"; then
    break
  fi
done
if ! grep -q "linux smoke edit" "${smoke_file}"; then
  fail_with_log "File/Save menu item did not persist edited text"
fi

right_click_window_point "tab context menu mouse smoke" 80 48
right_click_editor_context "editor context menu mouse smoke"
click_editor "refocus editor after context menu smoke"
click_file_menu_item "File/Save menu item after context menu focus smoke" 188
ensure_running "File/Save menu item after context menu focus smoke"
if ! grep -q "linux smoke edit" "${smoke_file}"; then
  fail_with_log "Context menu smoke lost the saved file content"
fi

click_editor "focus editor for Edit menu smoke"
activate_menu_item "Edit/Select All menu item" 1 6
activate_menu_item "Edit/Copy menu item" 1 4
activate_menu_item "Edit/Cut menu item" 1 3
activate_menu_item "Edit/Undo menu item after Cut" 1 1
activate_menu_item "Edit/Redo menu item after Undo" 1 2
activate_menu_item "Edit/Undo menu item after Redo" 1 1
activate_menu_item "Edit/Paste menu item" 1 5
sleep 0.3
click_editor "restore editor focus after Edit menu item smoke"
press_key "select all after Edit menu item smoke" ctrl+a
type_text "restore saved text after Edit menu item smoke" "seed"
press_key "restore saved text newline after Edit menu item smoke" Return
type_smoke_marker "restore saved edit marker after Edit menu item smoke"
click_editor "refocus editor before saving restored Edit menu text"
click_file_menu_item "File/Save menu item after Edit menu item smoke" 188
ensure_running "File/Save menu item after Edit menu item smoke"
for _ in $(seq 1 20); do
  if grep -q "linux smoke edit" "${smoke_file}"; then
    break
  fi
  sleep 0.1
done
if ! grep -q "linux smoke edit" "${smoke_file}"; then
  fail_with_log "Edit menu item smoke did not restore saved text"
fi

activate_menu_item "File/Open menu item" 0 2
cancel_modal "File/Open menu item"
activate_menu_item "File/Save As menu item" 0 5
cancel_modal "File/Save As menu item"
activate_menu_item "Text/Reopen Encoding menu item" 5 1
cancel_modal "Text/Reopen Encoding menu item"

# F10 focuses the menubar; Down opens File; Right walks the remaining top-level menus.
press_key "focus menubar" F10
press_key "open File menu" Down
for _ in $(seq 1 7); do
  press_key "walk top-level menus" Right
done
press_key "close menu" Escape

# Exercise non-destructive shortcut-backed menu actions through real UI input.
press_key "new tab shortcut" ctrl+n
press_key "find shortcut" ctrl+f
press_key "close find bar" Escape
press_key "replace shortcut" ctrl+h
press_key "close replace bar" Escape
press_key "command palette shortcut" ctrl+shift+p
press_key "close command palette shortcut" ctrl+shift+p
press_key "word wrap toggle on" alt+z
press_key "word wrap toggle off" alt+z
press_key "select all shortcut" ctrl+a
press_key "copy shortcut" ctrl+c
press_key "undo shortcut" ctrl+z
press_key "redo shortcut" ctrl+y
press_key "close clean tab shortcut" ctrl+w

# Exercise selected menu items through the actual menubar, not only shortcuts.
activate_menu_item "File/New menu item" 0 1
activate_menu_item "File/Close menu item after File/New" 0 6
activate_menu_item "Find/Find menu item" 2 1
type_text "Find entry text before Enter smoke" "linux"
press_key "Find entry Enter smoke" Return
press_key "close find bar after Find/Find menu item" Escape
activate_menu_item "Find/Replace menu item" 2 2
press_key "close replace bar after Find/Replace menu item" Escape
activate_menu_item "Find/Find Next menu item" 2 3
activate_menu_item "Find/Find Prev menu item" 2 4
activate_menu_item "Find/Find All menu item" 2 5
press_key "close find bar after Find/Find All menu item" Escape
activate_menu_item "View/Commands menu item" 3 1
press_key "close command palette after View/Commands menu item" ctrl+shift+p
press_key "command palette shortcut for Enter smoke" ctrl+shift+p
type_text "command palette filter before Enter smoke" "Line Numbers"
press_key "command palette Enter smoke" Return
activate_menu_item "View/Line Numbers menu item off" 3 2
activate_menu_item "View/Line Numbers menu item on" 3 2
activate_menu_item "View/Marks menu item on" 3 3
activate_menu_item "View/Marks menu item off" 3 3
activate_menu_item "View/Word Wrap menu item on" 3 4
activate_menu_item "View/Word Wrap menu item off" 3 4
click_editor "restore editor focus before submenu save smoke"
press_key "select all before submenu save smoke" ctrl+a
type_text "restore saved text before submenu save smoke" "seed"
press_key "restore saved text newline before submenu save smoke" Return
type_smoke_marker "restore saved edit marker before submenu save smoke"
click_editor "refocus editor before saving restored submenu text"
click_file_menu_item "File/Save menu item before submenu save smoke" 188
ensure_running "File/Save menu item before submenu save smoke"
for _ in $(seq 1 20); do
  if grep -q "linux smoke edit" "${smoke_file}"; then
    break
  fi
  sleep 0.1
done
if ! grep -q "linux smoke edit" "${smoke_file}"; then
  fail_with_log "Shortcut/menu smoke did not restore saved text before submenu checks"
fi
activate_submenu_item "View/Theme Light submenu item" 3 5 2
activate_submenu_item "View/Theme System submenu item" 3 5 1
activate_submenu_item "Text/Line Ends LF submenu item" 5 3 2
activate_submenu_item "Text/Line Ends CR submenu item" 5 3 3
activate_submenu_item "Text/Line Ends CRLF submenu item" 5 3 1
click_file_menu_item "File/Save menu item after Line Ends submenu smoke" 188
ensure_running "File/Save menu item after Line Ends submenu smoke"
sleep 0.5
for _ in $(seq 1 20); do
  if grep -q "linux smoke edit" "${smoke_file}"; then
    break
  fi
  sleep 0.1
done
if ! grep -q "linux smoke edit" "${smoke_file}"; then
  fail_with_log "Line Ends submenu smoke lost the saved file content"
fi
activate_submenu_item "Settings/Tab Size 2 submenu item" 6 2 1
activate_submenu_item "Settings/Tab Size 4 submenu item" 6 2 2
activate_submenu_item "Settings/Tab Size 8 submenu item" 6 2 3
activate_submenu_item "Settings/Tab Size 4 submenu item restore" 6 2 2
activate_menu_item "File/New menu item before tab menu smoke" 0 1
activate_menu_item "File/New second menu item before tab menu smoke" 0 1
activate_menu_item "Tabs/Move Left menu item" 4 1
activate_menu_item "Tabs/Move Right menu item" 4 2
activate_menu_item "Tabs/Close Others menu item" 4 4
activate_menu_item "Tabs/Close All menu item" 4 5
activate_menu_item "Text/Change Encoding menu item" 5 2
cancel_modal "Text/Change Encoding menu item"
activate_menu_item "Settings/Font menu item" 6 1
cancel_modal "Settings/Font menu item"
activate_menu_item "Help/About menu item" 7 1
press_key "close About dialog" Escape

stop_app

GDK_BACKEND="${J3TEXT_SMOKE_GDK_BACKEND:-x11}" XDG_CONFIG_HOME="${exit_config}" "${exit_exe}" >"${exit_log_file}" 2>&1 &
pid=$!

window_id=""
for _ in $(seq 1 200); do
  window_id=$(find_j3text_window || true)
  if [ -n "${window_id}" ]; then
    break
  fi
  if ! kill -0 "${pid}" 2>/dev/null; then
    fail_with_log "j3Text exited before an exit-smoke window appeared"
  fi
  sleep 0.1
done

if [ -z "${window_id}" ]; then
  fail_with_log "j3Text exit-smoke window was not found"
fi

if ! xdotool windowactivate --sync "${window_id}" >/dev/null 2>&1; then
  fail_with_log "failed to activate j3Text exit-smoke window"
fi
sleep 1.0
activate_exit_menu_item "File/Exit menu item" 7

GDK_BACKEND="${J3TEXT_SMOKE_GDK_BACKEND:-x11}" \
  XDG_CONFIG_HOME="${action_config}" \
  J3TEXT_LINUX_ACTION_SMOKE=1 \
  J3TEXT_LINUX_ACTION_SMOKE_REPORT="${action_report_file}" \
  J3TEXT_LINUX_ACTION_SMOKE_OPEN_PATH="${action_open_file}" \
  J3TEXT_LINUX_ACTION_SMOKE_SAVE_AS_PATH="${action_save_as_file}" \
  J3TEXT_LINUX_ACTION_SMOKE_NEW_WINDOW_PATH="${action_new_window_path_file}" \
  "${action_exe}" "${smoke_file}" >"${action_log_file}" 2>&1 &
pid=$!
for _ in $(seq 1 120); do
  if ! kill -0 "${pid}" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "${pid}" 2>/dev/null; then
  fail_with_log "Linux action smoke did not exit"
fi
if ! wait "${pid}"; then
  pid=""
  fail_with_log "Linux action smoke exited with failure"
fi
pid=""
if ! grep -q '^PASS: Linux action smoke completed' "${action_report_file}"; then
  fail_with_log "Linux action smoke did not report success"
fi
if ! grep -q '^steps=128$' "${action_report_file}"; then
  fail_with_log "Linux action smoke did not run the expected action count"
fi
if [ ! -f "${settings_file}" ]; then
  fail_with_log "Linux action smoke did not persist settings"
fi
for expected in \
  'tab_size = 8' \
  'theme = "light"' \
  'shortcut_close_tab = "ctrl+f4"'
do
  if ! grep -Fxq "${expected}" "${settings_file}"; then
    echo "settings file content:" >&2
    sed -n '1,80p' "${settings_file}" >&2
    fail_with_log "Linux action smoke did not persist ${expected}"
  fi
done

GDK_BACKEND="${J3TEXT_SMOKE_GDK_BACKEND:-x11}" \
  XDG_CONFIG_HOME="${action_config}" \
  J3TEXT_LINUX_ACTION_SMOKE=restore \
  J3TEXT_LINUX_ACTION_SMOKE_REPORT="${restore_report_file}" \
  "${action_exe}" "${smoke_file}" >"${restore_log_file}" 2>&1 &
pid=$!
for _ in $(seq 1 120); do
  if ! kill -0 "${pid}" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "${pid}" 2>/dev/null; then
  fail_with_log "Linux restore smoke did not exit"
fi
if ! wait "${pid}"; then
  pid=""
  fail_with_log "Linux restore smoke exited with failure"
fi
pid=""
if ! grep -q '^PASS: Linux restore smoke completed' "${restore_report_file}"; then
  fail_with_log "Linux restore smoke did not report success"
fi
if ! grep -q '^steps=3$' "${restore_report_file}"; then
  fail_with_log "Linux restore smoke did not run the expected check count"
fi

assert_no_severe_log "Linux UI smoke" "${log_file}"
assert_no_severe_log "Linux exit smoke" "${exit_log_file}"
assert_no_severe_log "Linux action smoke" "${action_log_file}"
assert_no_severe_log "Linux restore smoke" "${restore_log_file}"

echo "PASS: Linux menu and shortcut smoke completed"
