# Linux 메뉴 동등성 점검 기록

기준일: 2026-06-17

이 문서는 Windows 구현을 기준 동작으로 두고 Linux GTK4 backend를 비교한 결과와
수정 내용을 기록한다. 기준 구현은 `src/platform/win32.rs`, Linux 구현은
`src/platform/linux.rs`다.

## 검증 범위

- Windows 메뉴 구성과 단축키 라벨은 `MainMenuBuilder`와 Win32 context menu 생성
  코드를 기준으로 확인했다.
- Linux 메뉴 구성은 `gio::MenuModel`을 펼치는 단위 테스트로 검증했다.
- Linux 기능 동작은 공통 app/domain/infra 테스트와 Linux platform 단위 테스트로
  검증했다.
- Linux 앱 시작은 임시 `XDG_CONFIG_HOME`을 사용한 startup smoke로 GTK event loop
  진입까지 확인했다.
- `scripts/linux_menu_smoke.sh`는 Linux 앱을 phase별 임시 실행파일 복사본과 X11 GTK backend로
  실행한 뒤 `xdotool`로 창을 활성화하고 top-level 메뉴를 실제 키 입력으로 열어
  본다. 이어서 New/Close, Find/Replace, Command Palette, Word Wrap, Select All,
  Copy, Undo, Redo처럼 파일 dialog나 저장을 건드리지 않는 shortcut-backed 명령을
  실제 UI 입력으로 실행한다. File/New, File/Close, File/Exit, Find/Find,
  Find/Replace, Help/About은 menubar에서 직접 선택해 실행한다.
- Windows GUI 런타임에서 메뉴를 직접 클릭하는 수동/자동 UI 비교는 이 환경에서
  수행하지 못했다. 대신 Windows target compile check로 Windows 코드가 깨지지
  않았는지 확인했다.
- 2026-06-17 추가 점검에서는 현재 셸의 `GDK_BACKEND=wayland` 때문에 Linux 메뉴
  스모크가 Wayland backend로 실행되어 `xdotool` 창 검색이 실패하는 문제를
  재현했다. 스모크 스크립트는 검증 목적대로 기본 X11 backend를 강제하고,
  필요한 경우 `J3TEXT_SMOKE_GDK_BACKEND`로만 override하도록 수정했다. 또한
  `cargo run` 런처 지연과 `xdotool` 출력 형식 차이를 피하기 위해 먼저
  `cargo build --quiet`를 수행한 뒤 `target/debug/j3text`를 실행하고, 숫자
  window id만 선택하도록 보강했다.
- Linux 메뉴 모델 테스트는 기존 top-level/section 검사에 더해 Recent, Theme,
  Line Ends, Tab Size, Shortcuts 하위 메뉴 항목과 action name까지 Windows 메뉴
  순서 기준으로 펼쳐 검증한다. Recent는 Windows 메뉴 builder처럼 메뉴 생성 단계에서
  최대 10개만 표시되는지도 별도로 확인한다.
- 2026-06-17 추가 UI 스모크는 임시 파일을 시작 인자로 넘겨 파일-backed 문서가
  열리는지 확인하고, 창 크기 변경 뒤 재활성화, 실제 editor 클릭, 입력, File > Save
  메뉴 저장, 파일 내용 검증까지 수행한다. 창 검색은 앱 process id와 top-level window
  크기를 함께 확인해 GTK 내부/보조 window를 잘못 잡지 않도록 했다. 실제 텍스트 입력은
  editor 클릭으로 잡은 child focus를 유지하도록 활성 창에 보내며, top-level window
  대상으로 직접 주입하지 않는다.
- 같은 UI 스모크는 tab strip과 editor 본문에 실제 마우스 우클릭을 보내 context
  menu 경로가 crash나 focus 손상 없이 열리고 닫히는지도 확인한다. context menu
  항목 순서/라벨은 별도 menu model 테스트가 Windows 기준과 비교한다.
- Edit 메뉴의 Select All/Copy/Cut/Undo/Redo/Paste는 shortcut뿐 아니라 실제 menubar
  선택 경로로도 실행해, 편집/undo/clipboard 계열 메뉴가 Windows 메뉴 경로와 같은
  action으로 이어지는지 확인한다. 빠른 Copy/Cut/Paste 연속 메뉴 실행에서 Linux가
  이전 시스템 clipboard 값을 붙여 넣는 문제도 이 경로에서 재현했다.
- File/Close는 clean 탭을 만든 직후 실제 File 메뉴 항목으로 실행하고, File/Exit는
  별도 clean 앱 세션에서 강제 종료 대신 실제 File 메뉴 항목으로 실행해 process가
  정상 종료되는지 확인한다.
- Find 메뉴로 열린 find 입력칸에는 실제 텍스트를 입력한 뒤 Enter를 눌러 Windows처럼
  Find Next 경로로 이어지는지 확인한다. Escape 닫힘은 Windows key dispatch가 직접
  처리하는 find/replace 입력칸 범위에서 확인한다. Command Palette는 Windows와 같은
  토글 단축키 닫기와 실제 필터 입력 뒤 Enter로 선택 명령을 실행하는 경로를 확인한다.
- View/Theme, Text/Line Ends, Settings/Tab Size 같은 하위 submenu도
  실제 menubar keyboard navigation으로 선택해, stateful 메뉴 항목이 Windows 기준
  action과 연결되는지 확인한다. Undo/Redo 등 앞선 이벤트 smoke가 파일-backed 문서
  내용을 바꿀 수 있으므로 submenu 저장 검증 직전에는 기준 텍스트를 UI 입력으로 다시
  저장해 후속 검증 오염을 막는다.
- 같은 추가 점검에서 Linux 앱이 파일 경로 인자를 GTK `Application` argv로도 넘겨
  `This application can not open files.` 오류를 내는 문제, GTK window만 남고 Rust
  `MainWindow` 상태가 drop되어 메뉴/action이 동작하지 않는 문제, 상태 보존 뒤
  programmatic buffer/tab 갱신 중 signal 재진입으로 `RefCell already borrowed`
  panic이 나는 문제를 재현했다.
- 2026-06-17 추가 확장에서는 `scripts/linux_menu_smoke.sh`를 네 phase로 나눴다.
  첫 phase는 `xdotool`로 실제 메뉴/단축키/dialog 취소 경계를 실행하고, 두 번째
  phase는 clean 앱 세션에서 File/Exit 메뉴가 정상 종료로 이어지는지 확인한다.
  세 번째 phase는 `J3TEXT_LINUX_ACTION_SMOKE=1` 전용 실행으로 GTK `gio::SimpleAction`을
  직접 activate해 하위 메뉴 action까지 상태 변화와 함께 검증한다. 네 번째 phase는
  action phase와 같은 별도 임시 `XDG_CONFIG_HOME`으로 앱을 다시 띄워 저장된 설정이
  action state까지 복원되는지 확인한다. UI phase와 action/restore phase의 설정
  디렉터리는 분리해, 실제 submenu UI 검증에서 바뀐 설정이 결정론적 action smoke
  초기 상태를 오염시키지 않게 한다. 전용 smoke는 일반 실행에는 동작하지 않고
  report 파일에 PASS/FAIL과 실행/검증 목록을 남긴다.
- 같은 smoke는 shortcut capture 성공 경로, 중복 shortcut 확인창의 No/Yes 결과,
  `Open in New Window`의 전제/성공 경로와 launch 요청 path, 사라진 Recent 항목 제거,
  Open/Save As/Save/Font/Reopen Encoding/Change Encoding의 accept 결과, Help/About action,
  설정 저장 flush와 재실행 복원까지 검증한다. 새 창 spawn과 dialog accept 값은 smoke
  전용으로 결정론화해 재귀 실행과 brittle dialog 자동화를 피하고, 일반 실행에서는
  기존처럼 실제 프로세스와 native/GTK dialog를 사용한다.
- 같은 action smoke는 Windows `update_menu_checks` 기준과 맞춰 clean file-backed
  문서, Save As 직후, dirty edit 직후, background save 중/후, Close All 이후의
  주요 action enabled/disabled 상태도 확인한다. Windows/app 계약상 clean 문서와
  clean untitled 문서에서도 `Save`는 활성이고, background save 중에만 비활성이다.
- 같은 action smoke는 Windows dirty prompt의 Save/Discard/Cancel 결정을 Linux
  action에서 주입한다. `close-tab`은 Cancel 탭 유지, Save blocking save 뒤 닫기,
  Discard 저장 없이 닫기를 검증하고, `open`/`reopen-encoding`은 Cancel 원 상태 유지와
  Discard 후 원래 open/reload 작업 계속을 검증한다. `exit`도 창을 실제로 닫기 전
  shared 확인 경로를 직접 호출해 Cancel은 종료 중단, Discard는 종료 허용, Save는
  blocking save 뒤 종료 허용으로 이어지는지 확인한다.
- 스모크 성공 경로에서도 UI/action/restore 로그에서 panic, `RefCell already
  borrowed`, GTK fatal error, GTK argv open 오류 같은 심각 패턴이 남지 않았는지
  확인한다. AT-SPI 연결 경고처럼 자동화 환경에서 기능 실패가 아닌 경고는 별도
  추적 대상으로만 기록한다.

## 메뉴별 점검 결과

| 메뉴 | 기능 | Windows 기준 동작 | Linux 기존 동작 | 문제 여부 | 원인 | 수정 내용 | 재검증 결과 |
| -- | -- | -- | -- | -- | -- | -- | -- |
| File | New/Open/Recent | 같은 순서, 구분선, 단축키 라벨 표시 | 구분선 section과 일부 단축키 라벨 없음 | 문제 | GTK menu model이 Win32 menu 구조를 반영하지 않음 | `gio::Menu` section과 `command_menu_label` 적용 | `main_menu_*` 테스트 통과 |
| File | Recent 표시 개수 | 메뉴 builder 단계에서도 최근 파일은 최대 10개만 표시 | app 상태의 10개 정규화에만 기대고 Linux 메뉴 생성에는 별도 cap 없음 | 문제 | Windows `append_recent_files`의 방어적 표시 제한을 Linux `build_menu_model`이 직접 반영하지 않음 | Linux Recent 메뉴 생성에도 `MAX_RECENT_FILES` 제한 적용, 12개 주입 시 10개 라벨/action만 표시되는 단위 테스트 추가 | `recent_menu_caps_items_to_windows_limit` 통과 |
| File | Recent missing file | Recent 항목의 파일이 사라지면 탭을 열지 않고 recent 목록에서 제거하며 경고 안내 표시 | 사라진 recent 항목 실행 결과와 dialog severity 검증 부족 | 문제 | menu model/action 테스트만으로 파일 존재 검사, 목록 제거 side effect, Windows `MB_ICONWARNING` 표시 계약을 확인하지 못함 | action smoke에서 missing recent path를 주입하고 `recent-0` 실행 후 문서 수 유지, stale path 제거, 기존 recent 유지 확인. Linux 안내는 Windows와 같은 warning dialog로 변경 | `scripts/linux_menu_smoke.sh` action phase 통과 |
| File | Save/Save As | `Save`, `Save As...` 단축키 라벨 표시 | 라벨에 단축키 표시 없음 | 문제 | GTK accelerator와 메뉴 라벨 표시가 분리됨 | Windows처럼 라벨에 `Ctrl+...` 포함 | `main_menu_sections_match_windows_menu_order` 통과 |
| File | Open/Save/Save As accept | dialog에서 선택한 path로 열고 저장하며 Save As는 선택 encoding을 적용 | UI smoke는 dialog 취소와 File > Save 메뉴 저장만 확인 | 문제 | dialog accept 경로의 내부 상태/파일 결과 검증 부족 | action smoke에서 Open path, Save As target, Save background completion, UTF-8 BOM 저장 bytes, 저장 후 dirty 해제를 검증 | `scripts/linux_menu_smoke.sh` action phase 통과 |
| File | Save As 초기 파일명 | Windows `GetSaveFileNameW`는 빈 `lpstrFile` 버퍼로 시작해 현재 파일명을 미리 채우지 않음 | Linux는 file-backed 문서에서 현재 파일명을 Save As dialog에 미리 채움 | 문제 | GTK `FileChooserNative::set_current_name`을 현재 문서 path 기준으로 호출해 Windows dialog 초기 표시와 달라짐 | Linux Save As dialog에서 current name prefill 제거 | `scripts/linux_menu_smoke.sh`, `cargo check` 통과 |
| File | Save As overwrite prompt | Windows `GetSaveFileNameW`는 `OFN_OVERWRITEPROMPT`로 기존 파일 선택 시 `Confirm Save As` Yes/No 확인을 표시하고 No면 저장을 시작하지 않음 | Linux Save dialog가 overwrite confirmation을 보장하지 않아 GTK/backend 기본값에 따라 확인 없이 진행될 수 있음 | 문제 | GTK4 `FileChooserNative`/`FileChooser`에는 Windows flag에 대응하는 overwrite confirmation 공개 API가 없고, 선택 target 확인 뒤 별도 사용자 확인 경계가 필요함 | Save As 선택 target metadata를 캡처한 직후 기존 파일이면 Windows 문구/Yes-No 순서의 `Confirm Save As` 확인을 표시. No는 encoding dialog와 저장을 시작하지 않고, Yes는 기존 target expectation으로 안전 저장 계속 | `save_as_overwrite_confirmation_matches_windows_contract`, `scripts/linux_menu_smoke.sh` action phase 통과 |
| File | Close/Close All/Exit | Close는 단축키 라벨, 그룹 구분, Exit는 clean 상태에서 정상 종료 | 구분선 없음, Close/Exit 실제 메뉴 실행 검증 부족, File/Exit 실행 시 `RefCell already mutably borrowed` panic | 문제 | section 미사용, 스모크가 단축키/강제 종료 중심, GTK `window.close()`가 action callback 안에서 `close_request`를 동기 재진입 | File section 분리, File/Close와 File/Exit menubar smoke 추가, `close-tab` action smoke 추가, Exit close를 idle로 지연 | 메뉴 모델 테스트와 `scripts/linux_menu_smoke.sh` 통과 |
| File/Text | Dirty prompt Save/Discard/Cancel | dirty 문서 close/open/reload/exit 전에 Save는 저장 후 계속, Discard는 저장 없이 계속, Cancel은 원 작업 중단 | prompt의 결정 결과를 자동 검증하지 못함 | 문제 | dialog 선택을 UI 자동화로 안정적으로 반복하기 어려워 shared confirm 흐름의 분기 검증이 부족 | action smoke 전용 prompt 결정 주입으로 `close-tab` Save/Discard/Cancel, `open` Cancel/Discard, `reopen-encoding` Cancel/Discard를 실행하고 탭 수, dirty 상태, 저장 파일 내용, reload 결과를 검증 | `scripts/linux_menu_smoke.sh` action phase 통과 |
| File/Text/Settings | 확인 dialog 버튼 | Windows MessageBox는 Yes/No 질문을 `Yes`, `No` 순서로, dirty prompt를 `Yes`, `No`, `Cancel` 순서로 표시하고 기본 응답은 `Yes`. `MB_YESNO`에는 Cancel 분기가 없다 | Linux GTK dialog가 `No`, `Yes` 또는 `Cancel`, `Don't Save`, `Save` 순서로 표시될 수 있고, 창 닫기 응답이 별도 Cancel 흐름으로 남을 수 있음 | 문제 | GTK 관례에 맞춘 수동 버튼 배열이 Windows `MB_YESNO`/`MB_YESNOCANCEL` 표시 계약과 달랐고 기본 응답을 명시하지 않음 | Linux 확인 dialog 버튼 spec을 Windows 라벨/순서로 고정하고 첫 버튼을 기본 응답으로 설정. Cancel 버튼 없는 Yes/No dialog의 비버튼 응답은 No로 매핑 | `choice_dialog_button_specs_match_windows_message_boxes`, `scripts/linux_menu_smoke.sh` 통과 |
| Edit | Undo/Redo/Cut/Copy/Paste/Select All | editor context menu와 같은 명령/라벨, Copy/Cut 직후 Paste는 방금 선택한 text 기준, Paste 뒤 저장/상태는 최신 buffer 내용 기준 | context menu 표시 직전 상태 갱신 부족, 라벨 차이, menubar 직접 실행 검증 부족, GTK 기본 clipboard 명령이 빠른 연속 실행에서 이전 시스템 clipboard 값을 붙일 수 있음, GTK paste 완료 시점 뒤 상태 동기화 부족 | 문제 | shared action state가 stale일 수 있고 UI smoke가 shortcut 중심, GTK clipboard copy/cut/paste 경계가 비동기적으로 이어질 수 있으며 paste 완료 신호를 별도로 발생 | context menu 표시 전 `update_status`, `update_menu_checks`; 라벨/section 정렬; Edit 항목 menubar 실행 smoke 추가 후 파일 기준 text 복원; Linux Copy/Cut은 선택 text를 직접 `gdk::Clipboard::set_text`에 넣고 Cut은 선택 범위를 즉시 삭제; `paste_done`에서 app 상태 재동기화 | `context_menus_match_windows_menu_order`, `scripts/linux_menu_smoke.sh` 통과 |
| Find | Find/Replace/Next/Prev/All | Find All 포함 단축키 라벨 표시. Find 입력칸 Enter는 Find Next 실행 뒤 match selection을 표시하되 focus는 Find 입력칸에 남겨 반복 Enter를 허용하고, Escape는 find bar를 닫음 | 일부 라벨/section 차이, 입력칸 Enter 실제 UI 경로와 focus 유지 검증 부족 | 문제 | Win32 builder와 GTK model 구조 불일치, smoke가 메뉴 열기/닫기 중심. Linux search selection이 TextView focus를 다시 잡을 수 있음 | Find section과 라벨 정렬. actual UI smoke에 Find entry 입력 후 Enter 경로 추가. Linux search는 시작 focus가 Find 입력칸이면 selection/no-match 후 focus를 복원하고 action smoke에서 selection+focus 유지 검증 추가 | Linux platform tests, `scripts/linux_menu_smoke.sh` 통과 |
| Find | Replace One/All | Replace bar의 One/All은 현재 선택 또는 전체 문서에서 실제 치환을 수행 | Replace bar open만 확인하고 One/All 결과 검증 부족 | 문제 | action smoke가 replace-current/replace-all을 실행하지 않음 | action smoke에서 고유 토큰을 삽입한 뒤 `replace-current`와 `replace-all` 결과를 buffer/app 상태로 검증 | `scripts/linux_menu_smoke.sh` action phase 통과 |
| Find | Replace entry Enter | Windows는 Replace 입력칸 Enter에 replace-current를 직접 매핑하지 않음 | Linux는 Replace 입력칸 Enter가 replace-current 실행 | 문제 | Linux 전용 `connect_activate` | Replace 입력칸 activate 연결 제거 | `cargo test` 통과 |
| View | Commands/Line Numbers/Marks/Word Wrap/Theme | 그룹 구분, 체크 상태, Word Wrap 단축키 라벨, command palette에서 선택한 명령 실행, Marks 제한 초과 시 한계/파일 크기를 포함한 정보 안내 표시 | section/라벨/크기/테마 색상 차이, command palette 내부 실행 결과 검증 부족, Marks 제한 안내가 Windows보다 덜 구체적이고 Linux에서 warning dialog로 표시됨 | 문제 | GTK UI 상수와 palette 독립 설정, palette open/close 위주 smoke, Linux warning 문구와 severity가 Windows helper 문구/`MB_ICONINFORMATION`을 반영하지 않음 | section, 라벨, Windows UI 상수와 palette 반영; actual UI smoke에서 Command Palette 필터+Enter와 Theme Light/System 선택 추가; action smoke에서 Commands palette 필터 후 Line Numbers 명령 실행 결과 검증; Marks 제한 안내 문구를 Windows와 같은 MB 형식으로 정렬하고 information dialog로 표시 | 메뉴/테마/상태 테스트, `visible_whitespace_size_limit_message_matches_windows_text`, `scripts/linux_menu_smoke.sh` 통과 |
| View | Commands 체크 상태 | Command Palette 토글 직후 View > Commands 체크 상태가 즉시 반영됨 | Linux는 palette 표시만 바꾸고 action state 갱신은 다음 상태 갱신까지 늦을 수 있음 | 문제 | `toggle_command_palette`가 Windows와 달리 `update_menu_checks`를 호출하지 않음 | 토글 직후 `update_menu_checks`를 호출하고 action smoke에서 `command-palette=true/false`를 검증 | `scripts/linux_menu_smoke.sh` action phase 통과 |
| View | Commands 실제 키 경로 | Command Palette는 토글 명령으로 열고 닫으며, 필터 입력 뒤 Enter로 선택 명령 실행 | UI smoke가 Escape 닫힘까지 확인한다고 기록했지만 Windows key dispatch는 Escape 닫힘을 find/replace 입력칸에만 직접 매핑 | 문제 | 문서와 smoke label이 Windows 코드 근거보다 넓게 Command Palette Escape 닫힘을 암시 | Command Palette menubar smoke 닫힘을 `Ctrl+Shift+P` 토글로 바꾸고 문서 표현을 Windows 기준으로 정정 | `scripts/linux_menu_smoke.sh` 통과 |
| Tabs | Move/Close/Close Others/Close All | 경계 move는 명령은 존재하고 no-op 가능 | Linux는 일부 경계에서 메뉴 disabled | 문제 | enabled 조건이 Windows보다 엄격 | `document_id` 기준으로 Windows에 맞춤 | `cargo test` 통과 |
| Text | Reopen/Change Encoding, Line Ends | 문서가 있으면 Reopen Encoding 명령 제공 | Linux는 path가 없으면 disabled, Line Ends submenu 실제 메뉴 선택 검증 부족 | 문제 | 파일 경로 기준 enabled 조건, submenu UI smoke 부재 | `document_id` 기준으로 조정하고 Line Ends LF/CR/CRLF submenu menubar smoke 추가 | `cargo test`, `scripts/linux_menu_smoke.sh` 통과 |
| Text | Reopen/Change Encoding accept | 선택한 encoding으로 reopen하거나 다음 저장 encoding을 변경 | dialog 취소만 자동 확인 | 문제 | encoding dialog accept 경로의 상태 변화 검증 부족 | action smoke에서 Reopen=UTF-8, Change=UTF-8 BOM을 적용하고 문서 encoding 상태를 검증 | `scripts/linux_menu_smoke.sh` action phase 통과 |
| Text/File | Encoding dialog invalid input | manual encoding 입력은 validation status를 계속 표시하고, invalid OK/Enter는 `Encoding` 경고 뒤 dialog를 닫지 않아 수정/Cancel/창 닫기가 가능 | Linux는 invalid OK에서 `Unknown encoding.`을 표시하고 dialog를 닫아 Reopen/Change/Save As 흐름이 취소됨 | 문제 | GTK dialog가 Windows `confirm_encoding_dialog_input`처럼 validation label과 retry loop를 유지하지 않음 | Linux encoding dialog에 status label과 Windows 문구 helper를 추가하고 invalid OK는 dialog를 유지하도록 변경 | `encoding_dialog_validation_matches_windows_text`, `scripts/linux_menu_smoke.sh` 통과 |
| Settings | Font/Tab Size/Shortcuts | 같은 순서와 submenu 구성 | section 구분 차이 | 문제 | GTK menu section 미사용 | Settings section 정렬 | `remaining_main_menus_match_windows_menu_order` 통과 |
| Settings | Tab Size submenu | 2/4/8 spaces 하위 메뉴 선택으로 설정 변경 | submenu 실제 메뉴 선택 검증 부족 | 문제 | action smoke만으로 UI submenu navigation을 확인하기 어려움 | Tab Size 2/4/8 submenu menubar smoke 추가 후 4 spaces로 복원 | `scripts/linux_menu_smoke.sh` 통과 |
| Settings | Font accept | font dialog 선택 결과를 설정과 화면에 적용 | dialog 취소만 자동 확인 | 문제 | font accept 경로의 설정 반영 검증 부족 | action smoke에서 Font accept를 `Monospace 13`으로 결정론화하고 settings 반영을 검증 | `scripts/linux_menu_smoke.sh` action phase 통과 |
| Settings | Font size 선택값 | Windows `ChooseFontW`는 4~288pt 제한을 걸고 선택 결과를 가장 가까운 정수 pt로 반올림 후 clamp | Linux는 Pango font size를 정수 나눗셈으로 읽어 fractional size가 내림 처리될 수 있고 dialog 선택값 해석의 Windows 제한 회귀 테스트가 없음 | 문제 | Pango size unit을 point로 변환하는 Linux 전용 로직이 Windows `selected_point_size` 반올림/clamp 규칙을 명시하지 않음 | Linux FontChooser 결과 size 변환 helper를 추가해 `round nearest`와 4~288pt clamp를 적용하고 단위 테스트로 고정 | `font_size_from_pango_matches_windows_rounding_and_limits` 통과 |
| Settings | Shortcut submenu 구분 | 각 shortcut 하위 메뉴는 `Now: ...` 비활성 항목 뒤 separator를 두고 `Set...`, `Default`, `Off` 명령 그룹을 표시 | Linux shortcut 하위 메뉴는 네 항목을 같은 section에 붙여 표시 | 문제 | GTK menu model에서 Windows `append_separator` 구조를 section으로 나누지 않음 | `Now` 정보 section과 shortcut action section을 분리하고 nested menu model 테스트가 section을 검증하도록 변경 | `nested_main_menu_items_match_windows_menu_order` 통과 |
| Settings | Shortcut duplicate prompt | Capture나 Default로 이미 쓰는 단축키를 다른 명령에 지정하면 Yes/No 확인 후 Yes는 기존 소유자에서 제거하고 새 명령에 지정, No는 기존 설정 유지 | Linux는 Capture 중복만 확인하고 Default는 바로 적용할 수 있었으며, 중복 단축키 확인창 분기 검증도 부족 | 문제 | Linux `configure_shortcut`이 `ShortcutMenuAction::UseDefault`를 duplicate prompt 공통 경로 밖에서 처리했고, action smoke도 Capture 중복만 검증 | Capture/Default/Disable 모두 Windows와 같은 shortcut 결정 공통 경로로 정리. action smoke에서 capture 중복은 `CloseTab=Ctrl+F4` 상태의 `NewFile` Capture로, Default 중복은 다른 명령이 `CloseTab` 기본키를 소유한 상태의 `CloseTab` Default로 No/Yes를 모두 검증 | `scripts/linux_menu_smoke.sh` action phase 통과 |
| Settings | Shortcut keypad capture | Windows shortcut capture는 top-row `0..9`, `A..Z`, `F1..F24`만 받고 numpad `VK_NUMPAD*`는 단축키로 캡처하지 않음 | Linux는 `gdk::Key::to_unicode()` 경로로 `KP_1` 같은 keypad 숫자를 일반 숫자 단축키처럼 캡처할 수 있음 | 문제 | GDK keypad keyval도 unicode 숫자로 변환될 수 있는데 Windows virtual-key whitelist와 같은 keypad 제외 조건이 없었음 | Linux shortcut key mapping에서 `KP_` key name을 제외하고 단위 테스트 추가 | `gtk_key_capture_requires_safe_editor_shortcut` 통과 |
| Settings | Shortcut invalid-key warning owner | Windows shortcut capture 중 modifier 없는 문자/숫자 key를 누르면 capture 창을 owner로 둔 `Shortcut` warning을 표시 | Linux는 invalid-key warning을 main window에 transient로 붙여 capture dialog 뒤에 가려지거나 focus stack이 달라질 수 있음 | 문제 | Linux `capture_shortcut` key handler가 capture dialog가 아니라 `self.window`를 parent로 전달 | message dialog helper가 `gtk::Window` parent를 받을 수 있게 하고, shortcut capture invalid-key warning은 capture dialog를 transient parent로 사용 | `message_dialog_can_use_capture_dialog_as_transient_parent` 통과 |
| Edit shortcut | 반복 키 입력과 modifier release | Windows는 `WM_KEYDOWN` repeat bit만 무시하고 key release 순서에 따라 shortcut이 영구 차단되지 않음 | Linux는 눌린 shortcut 조합 전체를 저장해 `Ctrl+S` 후 Ctrl을 먼저 떼면 release 이벤트의 modifier 상태가 달라져 guard가 해제되지 않을 수 있음 | 문제 | repeat guard key가 물리 주 key가 아니라 modifier 포함 shortcut 조합이었음 | Linux repeat guard를 `ShortcutKey` 기준으로 저장/해제해 modifier release 순서와 무관하게 주 key release에서 해제 | `editor_shortcut_repeat_guard_releases_key_without_modifier_state` 통과 |
| Settings | Tab Size/Theme/Shortcut 저장과 복원 | 메뉴 설정 변경 후 다음 실행에서도 설정과 체크 상태 유지 | 저장 flush와 다음 실행 action state 복원 검증 부족 | 문제 | UI smoke가 한 실행 안의 상태 변경만 확인 | action smoke에서 실행파일 옆 settings TOML flush 후 restore smoke로 `tab-size-8`, `theme-light`, `CloseTab=Ctrl+F4` 복원 확인 | `scripts/linux_menu_smoke.sh` restore phase 통과 |
| Help | About | About dialog 명령 | 동일 명령이나 전체 메뉴 순서와 action 실행 검증 부족 | 문제 | 테스트 부재 | top-level/menu item 테스트, menubar smoke, action smoke 추가 | `main_menu_top_level_matches_windows_order`, `scripts/linux_menu_smoke.sh` 통과 |
| Help/Find/File | 정보 dialog 문구 | About, Find/Results no match, pending save 안내는 Windows title/text와 같은 정보 dialog | 회귀 방지 테스트가 error/file changed 중심이라 일반 정보 dialog 문구는 직접 고정하지 못함 | 보강 | 사용자 표시 문자열이 흩어진 리터럴로 남음 | Linux 정보 dialog title/text를 상수화하고 Windows 문자열과 맞는지 단위 테스트에 추가 | `message_dialog_titles_match_windows_text` 통과 |
| Help/Find/File/Settings/Text | message dialog 표시 구조 | Windows `MessageBoxW(owner, text, title, flags)`는 창 제목에 title을 두고 본문에는 message text만 표시 | Linux `MessageDialog`가 title을 본문 primary text로, message를 secondary text로 표시해 Windows보다 제목이 본문에 한 번 더 드러남 | 문제 | GTK `MessageDialog` builder에서 `.text(title).secondary_text(message)`를 사용해 Windows의 title/body 경계를 뒤집음 | 공통 `new_message_dialog` helper를 추가하고 `.title(title).text(message)`로 모든 info/warning/error/choice dialog를 생성. secondary text는 비워 Windows body 구조와 맞춤 | `message_dialog_layout_matches_windows_title_and_body_contract` 통과 |
| External File | 변경 감지 reload 질문 | 외부 변경 감지 시 `File Changed` 제목의 Yes/No warning dialog로 reload 여부 질문 | Linux는 `External Change` 제목을 사용 | 문제 | Linux 전용 제목 문자열이 Windows `message_box(..., \"File Changed\", MB_YESNO | MB_ICONWARNING)`과 달랐음 | reload 질문 제목을 `File Changed`로 고정 | `message_dialog_titles_match_windows_text` 통과 |
| Editor context | Undo/Redo/Cut/Copy/Paste/Select All | 현재 상태 기준 enabled, 단축키 라벨 | 표시 직전 상태 갱신 없음 | 문제 | stale action state 가능 | 표시 직전 상태 갱신 | context menu 테스트 통과 |
| Editor context | 실제 우클릭 표시/닫기와 focus 경계 | Windows는 editor context menu 표시 전 편집 컨트롤에 focus를 줌 | Linux는 context menu 표시 전 `TextView` focus를 명시하지 않아 다른 위젯 focus가 남을 수 있음 | 문제 | Win32 `SetFocus(self.edit)` 대응 경로가 GTK에 없음 | editor context menu 표시 전과 닫힘 후 idle 시점에 GTK window focus를 `TextView`로 지정. UI smoke는 우클릭 context menu 표시/닫기 후 명시 editor refocus와 저장 흐름이 깨지지 않는지 확인 | `scripts/linux_menu_smoke.sh` 통과 |
| Tab context | Open in New Window/Close/Close Others | 우클릭한 탭 선택 후 표시, 단일 탭이면 Open in New Window는 명령 없는 비활성 항목, 다중 탭이면 새 창 명령 활성. Open in New Window는 현재 file-backed 탭 path를 별도 프로세스 인자로 넘긴 뒤 원래 탭을 닫음 | 메뉴 모델 구조와 라벨/성공 경로 검증 부족, 단일 탭 Open in New Window가 action 없는 disabled item인지 테스트하지 않음, action smoke에서는 재귀 실행 방지를 위해 새 프로세스 spawn을 no-op 처리해 launch 요청 path 검증이 약함 | 문제 | 테스트 부재, 단일/다중 탭 context menu model 분기 누락, smoke 전용 launch 우회 | 단일 탭은 action 없는 disabled item, 다중 탭은 `win.open-new-window` action item으로 context menu model을 분기. action smoke에서는 새 창 launch 요청 path를 파일에 기록해 현재 탭 path와 일치하는지 검증 | context menu 테스트, action smoke, UI smoke 통과 |
| Tab context | Open in New Window 오류 처리 | 실행 파일 확인 실패/새 process spawn 실패 시 Windows와 같은 사용자 동작명 `open in new window`와 내부 context `find app executable`/`launch new window`를 유지 | Linux는 실패 context가 `resolve current executable`/`open new window`로 달라 사용자 오류 메시지와 로그가 Windows와 다를 수 있음 | 문제 | Linux 전용 launch helper의 오류 context 문자열과 helper 호출이 Windows 구현과 달랐음 | Linux launch helper를 Windows와 같은 `AppError::io_path_with_user` context로 정렬 | `cargo check` 통과 |
| Build/Test | Linux menu smoke | X11 기반 자동 입력에서 앱 창을 찾고 메뉴/단축키 실행 | 현재 셸의 `GDK_BACKEND=wayland`를 상속해 `xdotool`이 창을 찾지 못함 | 문제 | 스모크가 X11 backend를 보장하지 않고 `cargo run` 런처와 창 검색 결과를 직접 사용 | 기본 X11 강제, `J3TEXT_SMOKE_GDK_BACKEND` override 추가, 사전 build 후 바이너리 실행, window id numeric/class 검색 보강 | `scripts/linux_menu_smoke.sh` 통과 |
| Build/Test | File/Save·Exit smoke 안정성 | Save/Exit도 실제 File 메뉴에서 선택되어 파일 저장과 clean 앱 종료로 이어짐 | 키보드 탐색이나 루트 절대 좌표 클릭은 GTK popover focus 또는 window manager reparenting 상태에 따라 Save/Exit 대신 editor/탭 영역이나 다른 항목을 클릭할 수 있음 | 문제 | X11/GTK menubar keyboard focus와 popover item focus가 환경별로 흔들리고, `getwindowgeometry` 루트 좌표가 클라이언트 상대 메뉴 위치와 어긋날 수 있음 | File top-level과 표시된 Save/Exit 항목을 `xdotool --window` 창 상대 좌표로 실제 클릭해 저장/종료를 확인 | `scripts/linux_menu_smoke.sh` 통과 |
| Build/Test | Submenu save smoke 격리 | Line Ends 저장 검증은 현재 파일의 기준 텍스트에서 시작 | 앞선 shortcut/menu smoke가 Undo/Redo나 focus 차이로 buffer 내용을 바꾸면 Line Ends 저장 검증이 원인보다 늦게 실패 | 문제 | 긴 UI smoke phase가 같은 파일-backed 문서를 계속 공유 | Theme/Line Ends submenu 검증 직전에 editor 내용을 기준 텍스트로 다시 입력하고 저장해 이전 이벤트의 상태 오염을 제거 | `scripts/linux_menu_smoke.sh` 통과 |
| Build/Test | GTK UI 단위 테스트 안정성 | Linux GTK UI 검증은 Rust test harness의 병렬 test thread와 무관하게 한 GTK thread에서 안정적으로 실행되어야 함 | 전체 `cargo test`에서 GTK 초기화 테스트들이 서로 다른 Rust test thread에서 `gtk::init()`을 호출하거나 동시에 GTK 객체를 만들면 `Attempted to initialize GTK from two different threads` panic 또는 native segfault가 발생할 수 있음 | 문제 | 테스트가 `gtk::init().is_err()`만 확인해 GTK crate의 다른 스레드 재초기화 panic을 skip으로 처리하지 못하고, GTK UI 객체 생성 구간도 병렬 test thread에서 겹칠 수 있음 | 테스트 전용 `run_gtk_test_or_skip` helper를 `gtk::test_synced` 기반으로 바꿔 GTK UI 테스트 본문을 전용 GTK worker thread에서 직렬 실행. GTK 초기화 전 실패만 skip하고 테스트 본문 panic은 그대로 실패 처리 | `cargo test --lib -- --test-threads=1 --nocapture`, `cargo test --quiet` 재검증 |
| File | 시작 파일 열기 | 파일 경로 인자로 실행하면 해당 파일 탭을 열고 저장 가능 | Linux에서 `gtk::Application`이 argv를 파일 open 요청으로 해석해 시작 오류 발생 | 문제 | `run()`이 자체 startup path를 수집한 뒤에도 GTK에 원래 argv를 다시 전달 | GTK에는 앱 이름만 넘기도록 `run_with_args(&[APP_TITLE])` 사용 | 시작 파일 smoke에서 파일 로드/편집/저장 확인 |
| File/Edit | New, Save, Close 등 action 실행 | 창 생명주기 동안 Rust window state가 유지되어 menu/action이 계속 동작 | GTK 창은 보이지만 `MainWindow` state가 activate 종료 뒤 drop되어 startup restore와 action이 no-op | 문제 | Linux는 Windows처럼 native window user data에 state를 붙이는 보존 경로가 없었음 | `ApplicationWindow` qdata에 `Rc<RefCell<MainWindow>>` strong reference를 보관하고 close 승인 시 해제 | 시작 파일 smoke, New/Close/Find/About 메뉴 실행 통과 |
| File/Edit/View | startup restore와 programmatic UI 갱신 | 내부 갱신 중 발생하는 상태 신호와 modal dialog 중 editor key event가 앱을 panic시키지 않음 | 상태 보존 뒤 buffer undo notify, tab switch, timer, 저장 dialog가 열린 동안 들어온 key event에서 `RefCell already borrowed` panic | 문제 | GTK signal/key event 재진입 경로가 이미 mutable borrow 중인 `MainWindow`를 다시 borrow | signal/timer/error/editor key path에서 `try_borrow`/`try_borrow_mut`로 재진입을 건너뛰고 기존 흐름 유지 | 확장 Linux smoke 반복 통과 |
| View/Settings/Text/Tabs/Edit/File | 하위 메뉴와 stateful action | Theme, Tab Size, Shortcuts, Line Ends, tab move/close/edit action은 Windows 메뉴 action과 같은 상태 변경 | 실제 키보드 submenu drill-down은 GTK popover focus 방식 때문에 자동화가 불안정하고 상태 검증이 부족 | 문제 | X11 키 입력만으로 submenu page 진입/선택 결과를 안정적으로 증명하기 어려움 | `J3TEXT_LINUX_ACTION_SMOKE`에서 `gio::SimpleAction::activate()`로 Recent, Open in New Window, 전체 `theme-*`, `tab-size-*`, 전체 `shortcut-*-default/off/capture`, `line-ending-*`, tab/edit action을 실행하고 상태를 검증 | `scripts/linux_menu_smoke.sh` action phase 통과 |

## 비메뉴 동작 차이와 수정

| 영역 | Windows 기준 | Linux 차이 | 수정 내용 | 재검증 |
| -- | -- | -- | -- | -- |
| 기본 창 크기 | 800x600 | 900x650 | Linux 기본 크기를 800x600으로 변경 | startup smoke, `cargo check` |
| Find/Search/Command/Status 패널 | Windows 상수 높이/폭과 초기 표시 상태. Find bar는 Find 42, entry 최소 120, With 58, Next/Prev 52, One 68, All 42, List 70, X 26, control 높이 24를 사용. Search Results는 112px 영역 안에 좌우 8px 여백과 106px list 높이를 사용. Command Palette는 154px 높이 안에서 8px 여백, 24px filter, 6px 간격, 108px list를 사용. 상태바는 `SB_SETPARTS` 경계 기준으로 Line 140, Chars 120, Selected 130, Encoding/Line Ends/Wrap 110, Save state 130 폭을 사용 | GTK natural size와 다른 상수, 실제 위젯 request/visibility 검증 부족. Linux find bar 버튼/라벨, search result margin/list 높이, command palette 내부 여백/list 높이, 상태 라벨 폭이 Windows와 일부 어긋날 수 있음 | Linux 상수를 Windows 값에 맞추고 action smoke에서 find/search/command palette 초기 숨김, find bar child request, search result margin/content height, command palette margin/spacing/filter/list request, line number 표시, panel/status width request를 직접 검증. 상태바 폭 상수도 Windows 경계 기준으로 고정 | `cargo test`, `scripts/linux_menu_smoke.sh` |
| Status | Line/Column 표시 | Windows Rich Edit는 `EM_GETSEL`, `EM_LINEFROMCHAR`, `EM_LINEINDEX` 기준으로 selection start의 줄/열을 표시하며 column은 UTF-16 unit 기준 | Linux는 GTK `TextIter::line_offset()`을 그대로 써 한글/이모지처럼 UTF-16 unit 수와 Unicode scalar 수가 다른 문자 뒤에서 column이 Windows보다 작게 표시될 수 있음 | Linux status surface state 계산을 document text의 줄 시작 UTF-16 offset 대비 selection start UTF-16 offset으로 변경. 한글/이모지/CRLF 단위 테스트 추가 | `status_column_uses_windows_utf16_offsets`, Linux platform tests 통과 |
| Tabs | 파일 경로 툴팁 | Windows 탭 툴팁은 file-backed 문서의 전체 경로를 표시하고, 긴 경로를 80자 기준으로 path separator 우선 줄바꿈 | Linux 탭 툴팁은 전체 경로를 한 줄로 표시해 긴 경로에서 Windows보다 넓고 읽기 어려움 | Linux에 Windows와 같은 경로 줄바꿈 helper를 추가하고 tab 생성/갱신 시 적용. separator 우선 줄바꿈과 긴 segment 분할 단위 테스트 추가 | `long_tab_tooltip_*` Linux platform tests 통과 |
| Linux 창 제목 | Windows는 문서 제목이 비어도 `{title} - j3Text` 형식을 유지해 `j3Text - j3Text`로 표시 | Linux는 문서 없음 상태에서 `j3Text`만 표시 | Linux title helper를 Windows status title 포맷과 같게 변경하고 빈 제목/일반 제목 단위 테스트 추가 | `window_title_matches_windows_status_format` 통과 |
| Linux 줄 번호 갱신 | Windows는 현재 첫 visible line부터 최대 200줄만 줄 번호 텍스트를 만들고 같은 snapshot이면 갱신을 생략 | Linux는 전체 문서 줄 번호를 매번 생성하고 별도 scrolled adjustment를 동기화 | Linux도 첫 visible line 기준 최대 200줄 snapshot으로 갱신하도록 변경하고, buffer 교체 시 snapshot을 무효화 | `scripts/linux_menu_smoke.sh` layout/action smoke 통과 |
| 검색 결과 표시 | find bar가 열려 있을 때만 결과 표시 | 결과 panel만 남을 수 있음 | `show_find_bar && show_search_results` 조건 적용 | `cargo test` |
| 문서 로드 Undo history | 로드 직후 Undo로 이전 문서 내용 복귀 안 함 | GTK programmatic load가 undo history를 만들 수 있음 | irreversible action으로 buffer 교체 | `programmatic_buffer_load_clears_undo_history` 통과 |
| Undo clean 복귀 | dirty 표시 즉시 사라짐 | 탭 `*`가 남을 수 있음 | text command 뒤 탭 표시 항상 갱신 | `cargo test` |
| Modal dialog 중 timer | modal 동안 save/timer 작업 중지 | Linux timer가 계속 돌 수 있음 | modal depth guard 추가 | `cargo test`, startup smoke |
| 파일 dialog non-local path | local path 또는 오류 | Accept 후 path 없으면 조용히 취소 | 복구 가능한 오류 반환 | `cargo test` |
| Windows/Linux compile | Windows 코드 컴파일 유지, Linux metadata API도 같은 공통 계약 사용 | `has_change_marker()`가 Windows/test 전용이라 Linux 런타임의 metadata cache 정책에서 공통 helper를 쓰기 어려움 | `has_change_marker()`를 공통화하고 Windows/Linux target check로 컴파일 유지 확인 | Windows GNU/MSVC check 통과 |
| 저장 target 충돌 검사 | Windows/Linux 모두 저장 시점에 target expectation을 확인하고 같은 크기 외부 변경도 content fingerprint로 막는다 | Linux에서 수집한 `ctime`을 Windows `ChangeTime`처럼 확정 marker로 쓰면 빠른 같은 크기 외부 변경을 놓쳐 저장 시 외부 변경을 덮어쓸 수 있음 | background 외부 변경 poll은 제거하고, Linux `ctime`은 저장 충돌의 확정 unchanged marker로 쓰지 않으며 content fingerprint guard를 유지한다 | `linux_change_marker_does_not_skip_fingerprint_conflict_guard`, infra same-size conflict tests 통과 |
| Linux smoke 환경 | X11 GTK backend에서 `xdotool`로 창 활성화와 메뉴 입력 | Wayland backend 상속 시 창 검색 실패 | `J3TEXT_SMOKE_GDK_BACKEND`가 없으면 X11을 강제하고, 사전 build/숫자 window id 검색으로 안정화 | `scripts/linux_menu_smoke.sh` 통과 |
| Linux smoke 키 입력 | 반복 UI smoke 중 modifier 상태가 다음 입력에 새지 않아야 함 | 실패한 X11 입력 뒤 Ctrl/Shift 상태가 남으면 저장 단축키나 메뉴 이동이 흔들릴 수 있음 | `press_key`도 `xdotool type`처럼 `--clearmodifiers`를 사용해 각 키 입력 전에 modifier 상태를 정리 | `scripts/linux_menu_smoke.sh` 통과 |
| Linux resize/focus | 창 크기 변경과 focus 복귀 뒤에도 메뉴/편집/저장이 계속 동작 | UI smoke가 초기 크기에서만 메뉴와 단축키를 실행 | 스모크 초반에 640x480, 800x600 resize와 window reactivate를 수행한 뒤 저장/메뉴 시나리오를 이어서 실행 | `scripts/linux_menu_smoke.sh` 통과 |
| Linux text input smoke | 실제 editor focus 뒤 키 입력과 Save가 파일에 반영 | top-level window 대상 직접 type가 GTK child focus를 놓칠 수 있음 | `xdotool type --window` 대신 활성 창 입력으로 보내 editor 클릭으로 얻은 focus를 유지 | `scripts/linux_menu_smoke.sh` 통과 |
| Linux 시작 인자 | 자체 startup path 처리로 파일 탭 열기. 실행 파일명과 빈 인자는 startup path에서 제외 | GTK argv도 같은 파일을 처리하려 해 startup 오류가 났고, Linux startup arg 해석은 inline 처리라 Windows helper 테스트와 같은 회귀 방지가 약함 | `gtk::Application::run_with_args(&[APP_TITLE])`로 GTK argv를 분리하고, Linux도 Windows처럼 `startup_file_paths_from_args` helper와 실행 파일명/빈 인자 제외 단위 테스트 추가 | 시작 파일 smoke, `startup_file_paths_skip_executable_and_empty_args_like_windows` 통과 |
| Linux window state | 창이 닫힐 때까지 app state 유지 | GTK object만 살아 있고 Rust state가 해제됨 | window qdata에 state를 보관하고 close 승인 시 release | 시작 파일 restore/action smoke 통과 |
| Linux signal 재진입 | programmatic UI 갱신 중 중첩 이벤트가 crash를 만들지 않음 | undo notify/tab switch/timer/drop/error path에서 중복 mutable borrow 가능 | 재진입 가능한 signal path를 `try_borrow` 기반으로 정리 | `scripts/linux_menu_smoke.sh` 통과 |
| Linux 메뉴 모델 갱신 | 설정 메뉴 항목은 Windows처럼 즉시 상태가 반영되고 메뉴가 안정적으로 닫힘 | `View/Marks` 실제 menubar smoke 중 GTK action callback 안에서 `PopoverMenuBar` 모델을 즉시 교체하면 native segfault가 날 수 있음 | 설정 적용과 stale Recent 제거 뒤 메뉴 모델 재구성은 idle callback으로 미뤄 활성 menu popover가 닫힌 뒤 수행 | `scripts/linux_menu_smoke.sh` 재실행 통과 |
| Linux File/Exit | clean 상태의 Exit 메뉴는 정상 종료 | action callback이 `window.close()`를 직접 호출하면 `close_request`가 같은 `MainWindow`를 다시 mutable borrow해 panic | Exit action에서 close 요청을 idle callback으로 넘겨 borrow 해제 뒤 처리 | clean exit phase 포함 `scripts/linux_menu_smoke.sh` 통과 |
| Linux Paste 완료 | Paste 뒤 저장/상태 갱신은 최신 buffer 내용 기준 | GTK paste 완료가 command return 이후 발생할 수 있어 빠른 메뉴 연속 실행에서 상태 동기화가 늦을 수 있음 | `TextBuffer::connect_paste_done`에서 pending text를 app/domain에 재동기화 | Edit menubar smoke 포함 `scripts/linux_menu_smoke.sh` 통과 |
| Linux Copy/Cut 즉시성 | Copy/Cut 직후 Paste는 방금 선택한 텍스트를 사용 | 빠른 menubar Copy/Cut/Paste 연속 실행에서 이전 시스템 clipboard 값이 붙어 파일 내용이 바뀔 수 있음 | Copy/Cut을 선택 범위 text 기반으로 직접 구현해 clipboard를 즉시 갱신하고 Cut 삭제 뒤 app/domain 상태를 동기화 | Edit menubar smoke 포함 `scripts/linux_menu_smoke.sh` 통과 |
| Linux Paste plain text | Windows Rich Edit는 `CF_UNICODETEXT`로 plain text만 붙여넣음 | GTK `paste_clipboard`는 clipboard provider와 target 협상에 맡겨 rich/markup payload 우선 삽입 가능성을 코드상 배제하지 못함 | Linux Paste를 clipboard plain text read 후 명시적 `TextBuffer::insert`로 변경. local clipboard는 동기 적용하고 외부 clipboard는 비동기 read 완료 시 같은 document에만 삽입 | action smoke에서 Cut 뒤 Paste와 `text/html`+plain text mixed provider Paste가 plain text를 삽입하는지 확인, `scripts/linux_menu_smoke.sh` 통과 |
| Linux GTK 줄바꿈 sync | Windows 기준 문서 내용과 저장 line ending 정책이 분리되어야 하며, 편집 표면 동기화가 CRLF/CR/LF를 임의로 바꾸면 안 됨 | GTK `TextBuffer`가 CRLF/CR 텍스트를 보존하는지 회귀 테스트가 부족 | `TextBuffer` roundtrip 테스트를 추가해 CRLF/CR/LF 혼합 텍스트가 그대로 app/domain 분석으로 들어가는지 확인 | `gtk_text_buffer_preserves_line_endings_for_domain_sync` 통과 |
| Linux POSIX replace race | 기존 파일 저장은 Windows의 target expectation처럼 교체 시점에도 기존 target이 있어야 성공 | `rename(temp, target)`은 최종 확인 직후 target이 삭제되면 새 target을 생성할 수 있음 | target 존재가 필요한 Linux 저장은 `renameat2(RENAME_EXCHANGE)`로 temp와 target을 원자 교환하고 temp path에 남은 이전 target을 제거. target이 없으면 `ExternalFileChanged`로 실패 | `replace_existing_file_atomically_*linux*` 테스트 통과 |
| Linux action smoke | 메뉴 action이 실제 GTK action callback으로 상태를 변경하고 Windows 기준 enabled/check 상태를 유지 | 하위 메뉴와 dialog accept/dirty prompt/recent stale/shortcut duplicate/action accelerator 갱신, layout visibility는 키 입력 smoke만으로 내부 상태 변경/상태 표시 검증이 약함 | env 전용 action smoke를 추가해 128회 action/check를 수행하고 action enabled 상태, layout/status request, Command Palette check state, 설정/문서 수/편집 결과/dialog accept/dirty prompt(close/open/reload/exit)/recent stale/shortcut duplicate/GTK accelerator 갱신/설정 파일 flush/rich clipboard plain-text paste를 검증 | `scripts/linux_menu_smoke.sh` 통과 |
| Linux runtime log | 메뉴 실행 중 panic/fatal log가 없어야 함 | 성공 exit만 보고 심각 로그 패턴을 별도 검사하지 않음 | 스모크 성공 경로에서 UI/action/restore 로그의 panic, `RefCell already borrowed`, GTK fatal, GTK argv open 오류 패턴을 검사 | `scripts/linux_menu_smoke.sh` 통과 |
| Linux error dialog title | 복구 불가능하거나 사용자에게 표시되는 오류 dialog 제목은 `j3Text Error` | Linux 공통 오류 dialog 제목이 `j3Text` | Windows `show_error`의 `j3Text Error` 제목을 Linux `show_error_dialog`에도 적용 | `message_dialog_titles_match_windows_text` 통과 |
| Linux Marks 자동 비활성화 | 큰 문서 로드 중 Marks를 normal view로 자동 보정해도 사용자 설정 파일은 덮어쓰지 않음 | 자동 보정 경로에서도 settings persistence를 예약할 수 있음 | 자동 보정은 `EditorApp` 런타임 상태만 바꾸고 저장 예약을 하지 않도록 분리 | `oversized_document_load_disables_marks_without_persistence_boundary` 통과 |
| Linux 설정 복원 | 저장된 메뉴 설정이 다음 실행의 앱 설정과 체크 상태로 복원 | 저장 파일 존재만 확인하고 action state 복원을 확인하지 못함 | restore smoke를 추가해 같은 실행파일 옆 settings TOML로 재실행 후 `tab-size-8`, `theme-light`, `CloseTab=Ctrl+F4`를 검증 | `scripts/linux_menu_smoke.sh` restore phase 통과 |

## 실행한 검증

```text
cargo fmt --check
git diff --check
cargo check
cargo test platform::linux::tests -- --nocapture
cargo test
J3TEXT_SKIP_WINDOWS_RESOURCE=1 cargo check --target x86_64-pc-windows-gnu
J3TEXT_SKIP_WINDOWS_RESOURCE=1 cargo check --target x86_64-pc-windows-msvc
bash -n scripts/linux_menu_smoke.sh
scripts/linux_menu_smoke.sh
cargo clippy --all-targets --all-features -- -D warnings
```

추가로 임시 `XDG_CONFIG_HOME`에서 Linux startup smoke를 실행해 GTK event loop 진입을
확인했다.

2026-06-17 추가 검증:

```text
cargo fmt --check
git diff --check
cargo check
cargo test platform::linux::tests -- --nocapture
cargo test platform::linux::tests::choice_dialog_button_specs_match_windows_message_boxes -- --nocapture
cargo test platform::linux::tests::encoding_dialog_validation_matches_windows_text -- --nocapture
cargo test platform::linux::tests::message_dialog_titles_match_windows_text -- --nocapture
cargo test platform::linux::tests::recent_menu_caps_items_to_windows_limit -- --nocapture
cargo test platform::linux::tests::visible_whitespace_size_limit_message_matches_windows_text -- --nocapture
cargo test platform::linux::tests::oversized_document_load_disables_marks_without_persistence_boundary -- --nocapture
TMPDIR=<project temp dir on /home> cargo test
J3TEXT_SKIP_WINDOWS_RESOURCE=1 cargo check --target x86_64-pc-windows-gnu
J3TEXT_SKIP_WINDOWS_RESOURCE=1 cargo check --target x86_64-pc-windows-msvc
bash -n scripts/linux_menu_smoke.sh
scripts/linux_menu_smoke.sh
cargo clippy --all-targets --all-features -- -D warnings
```

2026-06-17 추가 검증에서는 `/tmp` tmpfs가 99% 사용 중인 환경에서 large-file
regression이 임시 파일을 쓰다가 `No space left on device`로 한 번 실패했다.
프로젝트 파티션의 임시 디렉터리를 `TMPDIR`로 지정해 재실행했을 때 전체 테스트가
통과했다. `cargo clippy --all-targets --all-features -- -D warnings`도 GTK
deprecated API 사용 지점의 의도를 국소 `allow`로 표시하고 기존 style lint를
정리한 뒤 통과했다.

최종 재실행 중 `View/Marks` menubar smoke에서 GTK native segfault를 한 번 재현했다.
원인은 설정 적용 경로가 활성 menu action callback 안에서 `PopoverMenuBar` 모델을
즉시 교체하던 점이었다. 설정 적용과 stale Recent 제거 후 메뉴 모델 재구성을 idle
시점으로 미룬 뒤 같은 `scripts/linux_menu_smoke.sh`를 다시 실행해 통과했다.

확장된 `scripts/linux_menu_smoke.sh`는 추가로 action smoke phase에서 128회 action/check를
실행하고 검증했다: `recent-0`, `open-new-window`, `open`, `choose-font`,
`reopen-encoding`, `change-encoding`, `save-as`, `save`, `command-palette`,
`line-numbers`, `visible-whitespace`, `word-wrap`, `tab-size-2/4/8`, 전체 `theme-*`,
전체 `shortcut-*-disable/default`, `shortcut-*-capture`, `find`,
`find-next`, `find-previous`, `find-all`, `replace`, `replace-current`, `replace-all`,
`close-find`, `new`, `tab-left`,
`tab-right`, `close-other-tabs`, `close-all-tabs`, `close-tab`, `line-ending-lf/cr/crlf`,
`undo`, `redo`, `select-all`, `copy`, `cut`, `paste`, `about`. Paste는 앱 내부 text clipboard뿐
아니라 `text/html`과 plain text가 함께 있는 GTK provider에서도 Windows처럼 plain text만
삽입하는지 확인한다. dialog-backed action은 smoke 전용
accept 값을 사용해 Open target path, Open in New Window launch 요청 path,
Font setting, Reopen/Change Encoding 결과,
Save As 기존 target의 overwrite No 취소/Yes 저장, Save As target과 UTF-8 BOM bytes,
Save background completion을 확인한다. 사라진 Recent
항목은 `recent-0` action으로 실행해 문서 수를 유지한 채 stale path를 제거하는지 확인한다.
마지막 상태는
`tab_size = 8`, `theme = "light"`, `shortcut_close_tab = "ctrl+f4"`로 flush해
실행파일 옆 settings TOML에 저장됐는지 확인했다. shortcut 중복 확인은 `NewFile` capture에
Ctrl+F4를 주입해 No이면 `CloseTab` 소유를 유지하고 Yes이면 `NewFile`로 이동하는지
확인한다. 이어서 다른 명령이 `CloseTab` 기본키를 소유한 상태를 만든 뒤 `CloseTab`
default 복원에서도 No/Yes 결과가 Windows와 같은지 확인했다.
shortcut 설정 변경 뒤에는 GTK application accelerator도 직접 읽어서 `win.new`,
`win.close-tab`이 기본/캡처/중복 이동/복원 상태와 맞는지 확인하고, Windows처럼
editor surface에서 직접 처리해야 하는 `win.copy`에는 application accelerator가
비어 있는지도 확인했다.
dirty prompt는 `close-tab` action에서
Cancel/Save/Discard를 각각 실행해 탭 유지, blocking save 뒤 닫기, 저장 없이 닫기를
확인했고, `open`/`reopen-encoding` action에서 Cancel 원 상태 유지와 Discard 후
open/reload 계속도 확인했다. `exit` dirty prompt는 shared close-request 확인 경로를
직접 호출해 Cancel/Discard/Save 결과와 Save 시 파일 반영을 확인했다. 이어서 restore smoke phase가 같은
실행파일 옆 settings TOML로 앱을 다시 실행해 `tab-size-8`, `theme-light`,
`shortcut_close_tab = "ctrl+f4"`가 앱 설정과 GTK action state에 복원되는지 확인했다.
action smoke와 restore smoke는 각각 `steps=128`, `steps=3`을 검증해 실행 범위가
의도치 않게 줄지 않도록 했다.
추가로 action smoke는 `save`, `save-as`, close/edit/find/text/tab action의 enabled
상태를 clean/dirty/saving/close-all 주변 상태에서 검증하고, command palette 내부
명령 실행, Find 입력칸 focus 유지, Replace One/All의 실제 치환 결과도 확인한다.

추가로 설치한 도구는 없다. 검증은 기존 Rust toolchain, GTK runtime, Git, shell
도구만 사용했다.

## 남은 확인 필요

- Windows와 Linux 실제 GUI에서 모든 메뉴 항목을 직접 클릭하고 스크린샷/상태를
  비교하는 자동화는 아직 없다. 현재 자동화는 Linux 창 resize/reactivate,
  tab/editor context menu 우클릭 smoke, top-level 메뉴 열기, File Close/Exit와
  Edit 항목 menubar 실행, Theme/Line Ends/Tab Size submenu 선택,
  일부 dialog/menu 항목 직접 선택과 취소,
  dialog accept action 결과 검증,
  shortcut-backed 명령 smoke, GTK action smoke, shortcut capture 주입 검증, 설정 파일
  저장/복원 검증, 코드 기반 menu model/action label 검증, 하위 메뉴 action snapshot
  검증까지다.
- Linux startup smoke에서 AT-SPI 연결 경고와 GTK slider size 경고가 출력됐다. 기능
  실패는 아니지만 UI 자동화 환경에서는 별도 추적이 필요하다.
- rich/HTML clipboard payload paste는 GTK mixed provider smoke로 Windows plain-text paste
  기준을 자동 검증한다. 다만 실제 브라우저/오피스 등 외부 앱 clipboard provider 조합별
  수동/자동 시나리오 테스트는 아직 필요하다.
- Windows 실기기 GUI와 외부 앱 clipboard provider 조합을 제외한 코드/스모크 기준의
  남은 Linux 전용 항목은 현재 문서화된 범위에서 정리했다.
