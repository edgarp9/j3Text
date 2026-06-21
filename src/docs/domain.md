# j3Text 도메인 계약

## 확정된 제품 경계

- j3Text는 Windows와 Linux를 지원하는 탭 기반 plain text editor다. 도메인 본문은
  `Document.content`의 plain Unicode text이며, 문서 내용으로 서식, RTF,
  embedded object, OLE 데이터를 갖지 않는다.
- Windows의 Rich Edit Control과 Linux의 GTK4 `TextView`는 주 편집 표면을
  구현하기 위한 platform 세부 수단이다. Windows 구현은 `platform::run()`에서
  `Msftedit.dll`을 로드하고, `MainWindow::create_controls`가
  `create_editor_text_surface`를 통해 `RICHEDIT50W` 컨트롤을 만든다. Linux 구현은
  `src/platform/linux.rs`에서 GTK4 `ApplicationWindow`, `TextBuffer`, `TextView`,
  `PopoverMenuBar`를 생성한다.
- Windows 주 Rich Edit Control은 비어 있는 상태에서 `EM_SETTEXTMODE`와
  `TM_PLAINTEXT`로 plain text mode를 설정한 뒤에만 문서 텍스트를 싣는다.
  이 설정이 실패하면 rich-text mode로 계속 실행하지 않고 시작 또는 컨트롤
  생성을 실패시킨다.
- 이 앱은 RTF 편집 기능을 제공하지 않는다. 저장, 열기, 검색,
  clipboard 삽입 경로는 plain text만 다룬다. Rich Edit 메시지와 구조체는
  Win32 `EditorTextSurface` 어댑터 내부 구현이고, app/domain 경계에는
  `get_text`, `set_text`, `set_readonly`, `select_all`, `undo`, `redo` 같은
  plain text 표면 동작만 노출한다.
- Rich Edit 색상 적용이나 GTK CSS theme 같은 presentation 처리는 platform
  책임이다. 이것은 문서 서식이 아니며 app/domain 계약이나 저장 결과로
  올라오지 않는다.
- `find_edit`, `replace_edit`, command filter, line-number control 같은 보조
  컨트롤은 일반 Win32 `"EDIT"` 컨트롤로 유지한다. Rich Edit 전환 범위는 주
  편집 표면인 `MainWindow.edit`뿐이다.

## 배포 및 Windows 호환성 기준

- 프로젝트 라이선스는 `Cargo.toml`에 `GPL-3.0-or-later`로 선언하고, 전체 GPL-3.0
  본문은 `LICENSE`에 둔다. Rust crate 의존성, build/proc-macro 의존성,
  Rust standard library notice, Material Icons 기반 앱 아이콘, OS/native runtime
  확인 항목은 `THIRD_PARTY_NOTICES.txt`와 `third_party_licenses/`에 둔다. 각
  외부 항목은 이름, 버전, 라이선스, 저작권/attribution 문구, 원본 URL, 라이선스
  파일 위치, 배포 포함 여부를 기록한다. `THIRD_PARTY_NOTICES.txt` 하단에는
  `LICENSE`, `COPYING`, `UNLICENSE`, `third_party_licenses/`에서 확인한 라이선스
  전문을 중복 제거해 함께 싣는다. release 빌드 경로를 여는
  `build_release.py`는 `LICENSE`, `THIRD_PARTY_NOTICES.txt`, `about.txt`,
  `third_party_licenses/`를 release 디렉터리에 복사하고
  `j3text-<version>-corresponding-source.zip`과 target별 binary zip을 생성해
  GPL-3.0-or-later 바이너리 배포에 필요한 고지와 Corresponding Source 제공
  경로가 누락되지 않게 한다. 배포 산출물에서 GPL 본문과
  MIT/Apache/BSD/Unicode/Apache-2.0 asset 계열 고지가 누락되지 않게 한다.
  Linux에서 GTK 계열 native library를 함께 번들링하는 배포 방식은 Rust crate
  고지와 별도로 해당 native library 및 그 transitive shared library 라이선스
  확인이 필요하다.
- About dialog의 창 제목은 `About j3Text`이고, 상단 버전 라벨은
  `j3Text <package version>`으로 표시한다. 버전 값은 Cargo package version에서
  자동으로 가져온다. 본문은 고정 크기의 읽기 전용 스크롤 영역으로 제공하며,
  실행 파일 옆의 `about.txt`를 우선 읽고 없으면 빌드에 포함된 같은 내용을 보여
  준다. 하단 왼쪽의 프로젝트 URL 버튼은 기본 브라우저로 URL을 열고, 하단
  오른쪽의 `OK` 버튼은 dialog를 닫는다. About dialog의 기준 크기는 450 x 400으로
  유지한다. 긴 라이선스 문구가 있어도 About dialog 전체 폭과 높이를 키우지 않는다.
- 현재 배포 최소 기준은 Windows 10 version 1709 이상이다. 저장 경계가
  `SetFileInformationByHandle`의 `FileRenameInfoEx`/POSIX rename 의미에 기대어
  열려 있는 target 파일도 보수적으로 교체하려고 하기 때문이다. 더 낮은 Windows
  버전을 지원하려면 atomic replace 경계에 별도 fallback 설계와 회귀 테스트가
  필요하다.
- 주 편집 표면은 Microsoft Rich Edit 4.1 (`Msftedit.dll`,
  `MSFTEDIT_CLASS`/`RICHEDIT50W`)에 의존한다. 앱은 시작 시 System32의
  `Msftedit.dll`을 로드하고, 주 편집 컨트롤을 `RICHEDIT50W`로 만든다. 이 DLL
  또는 클래스가 없거나 호환되지 않으면 rich edit 이전 버전으로 fallback하지 않고
  시작을 실패시킨다.
- Rich Edit 컨트롤은 빈 상태에서 `EM_SETTEXTMODE`와 `TM_PLAINTEXT`를 성공적으로
  적용해야 한다. 실패하면 RTF/rich-text mode로 계속 실행하지 않고 사용자에게
  plain text editor를 시작할 수 없다는 메시지를 보여 준다.
- 콘솔이 없는 Windows GUI 배포에서도 fatal startup error는 메시지 박스로 표시한다.
  내부 로그/표시 가능한 stderr에는 Win32 context와 error code를 남기고,
  사용자 메시지는 `Msftedit.dll` 로드 실패, editor control 생성 실패, plain text
  mode 설정 실패를 서로 다른 안내 문구로 구분한다.
- 참고 기준: Microsoft Learn의 Rich Edit 문서는 Rich Edit 4.1이 `Msftedit.dll`과
  `MSFTEDIT_CLASS`를 사용한다고 설명하고, `LoadLibraryExA`의
  `LOAD_LIBRARY_SEARCH_SYSTEM32`는 Windows XP/Server 2003에서 지원되지 않는다고
  명시한다. Windows 파일 rename flag의 `FileRenameInformationEx`는 Windows 10
  계열 API 경계로 취급한다.

## 플랫폼 백엔드 계약

- `src/platform/mod.rs`가 OS별 UI backend를 선택한다. Windows는 기존
  `src/platform/win32.rs` Win32/Rich Edit 구현을 유지하고, Linux는
  `src/platform/linux.rs` GTK4 구현을 사용한다.
- `src/main.rs`는 계속 `platform::run()`만 호출한다. Windows의
  `windows_subsystem = "windows"` 속성은 Windows release 빌드에만 적용한다.
- app/domain/infra는 OS UI 타입을 알지 않는다. 두 backend는 같은
  `EditorApp`, `Document`, `CurrentEditorStatus`, `FileDocumentIo`,
  `UserDataStore`, `TextEncoding`, `LineEnding`, `EditorCommandId` 계약을
  사용한다.
- Windows 전용 시스템 호출은 `cfg(windows)` 아래에 둔다. Linux는 GTK4 UI와
  표준 파일 API, `encoding_rs` 기반 code page 변환을 사용한다.
- 사용자 설정은 실행파일과 같은 디렉터리의 `<실행파일명>.toml` 파일에 저장한다.
  예를 들어 `j3text.exe` 또는 `j3text`로 실행되면 `j3text.toml`을 사용한다.
  설정 로드는 현재 TOML 포맷만 지원하고 이전 포맷 또는 제거된 키를 별도로
  마이그레이션하지 않는다.
  최근 파일은 OS 관례를 따른다. Windows는 `%APPDATA%`, Linux는
  `$XDG_CONFIG_HOME` 또는 `$HOME/.config` 아래의 `j3Text` 디렉터리를 사용한다.
- 플랫폼별 차이가 필요한 항목은 backend 내부에서만 흡수한다. 예를 들어 Windows는
  Rich Edit의 UTF-16/CR offset 변환을 처리하고, Linux는 GTK `TextBuffer`의 문자
  offset을 app/domain의 byte/UTF-16 offset으로 변환한다.

## Windows/Linux 기능 동등성 매트릭스

아래 항목은 Windows 구현을 기준 동작으로 두고 Linux GTK4 backend가 따라야 하는
사용자 기능 계약이다.

| 기능 영역 | Windows 기준 | Linux GTK4 구현 |
| --- | --- | --- |
| 앱 시작 | 설정/최근 파일 로드, 시작 경로 열기, 없으면 새 문서 | 동일. GTK activate 후 idle 단계에서 시작 경로를 처리 |
| 새 문서/탭 | UTF-8, CRLF, clean, writable untitled tab | 동일한 `EditorApp::new_document` 사용 |
| 파일 열기 | Text/All native file filter, 자동 인코딩 감지, 큰 파일 확인, read-only policy, 최근 파일 기록 | GTK native dialog에 같은 Text/All filter를 적용하고 동일한 `FileDocumentIo::load_with_metadata_and_prechecked_len` 사용 |
| 시작 경로 없음 | 생성 여부 질문 후 해당 path를 가진 clean 문서 생성 | 동일 |
| 저장/다른 이름 저장 | 직접 저장은 background save, dirty prompt 저장은 blocking save, Save As는 Text/All filter와 encoding 선택 | GTK Save As dialog에 같은 Text/All filter를 적용하고 동일한 save expectation과 metadata 검사 사용 |
| 원자적 저장 | Windows는 Win32 replace 경계 사용 | Linux는 같은 `FileDocumentIo` 정책과 POSIX rename 기반 replace 사용 |
| dirty prompt | close/open/reload/exit 전에 Save/Discard/Cancel | 동일 |
| 탭 조작 | 이동, 닫기, 나머지 닫기, 전체 닫기, 새 창으로 열기, 탭 context menu | 동일 명령 제공. GTK 탭 context menu는 우클릭한 탭을 선택한 뒤 표시하고, 단일 탭의 Open in New Window는 Win32처럼 명령 없는 비활성 항목으로 표시 |
| 편집 | Undo, Redo, Cut, Copy, Paste, Select All | GTK `TextBuffer` undo/clipboard API로 동일 명령 제공 |
| 검색/치환 | find bar, next/prev, find all list, replace one/all | 동일한 domain search range와 GTK selection 변환 사용 |
| 인코딩 | Reopen Encoding, Change Encoding, Save As encoding 선택, 수동 alias 입력 | 동일한 `TextEncoding::from_user_input`과 encodable 검증 사용 |
| 줄바꿈 | CRLF/LF/CR 저장 정책 변경, 상태 표시 | 동일한 `LineEnding` 계약 사용 |
| 보기 | line numbers, visible whitespace, word wrap | GTK line-number view, 표시용 whitespace render cache, wrap mode로 제공 |
| 테마/폰트/탭 크기 | 설정 저장, 테마 7종, font picker, tab size 2/4/8 | GTK CSS, font chooser, font metric 기반 tab stops로 제공 |
| 단축키 | 기본 단축키, 사용자 변경/기본값/비활성 | GTK accelerator 갱신과 같은 `EditorShortcuts` 저장 사용 |
| 메뉴 상태 | 메뉴 라벨, 체크 표시, 비활성 표시, 탭 context menu는 Win32 현재 메뉴를 기준으로 표시 | GTK `gio::Menu`와 stateful action으로 같은 체크/비활성 상태를 표시. 설정 변경 시 메뉴 모델을 재생성해 동적 라벨도 갱신 |
| 단축키 변경 | shortcut dialog는 실제 키 입력을 캡처하고 중복 단축키는 사용자 확인 후 이동 | GTK key event capture와 같은 중복 확인 흐름 사용 |
| 상태바 | line/col, chars, selected, encoding, line ending, wrap, save state, path/title | 동일 필드를 `CurrentEditorStatus`에서 읽어 표시 |
| 최근 파일 | 메뉴 표시, 최대 10개 표시, 사라진 파일 제거 | 동일. 메뉴 모델은 최근 파일 변경 시 재생성하고 Windows와 같은 10개 표시 제한을 적용 |
| 설정 복원 | 저장된 tab size, theme, shortcut 설정이 다음 실행의 메뉴 체크/action state에 반영 | 같은 `EditorSettings`를 로드하고 GTK stateful action 초기값과 메뉴 모델에 반영 |
| 저장 충돌 감지 | 저장 시점에 target snapshot을 비교하고 외부 변경이면 Reload/Save As/Cancel 제공 | 동일한 `FileDocumentIo` save expectation과 metadata/snapshot 비교 사용 |
| Drag & Drop | dropped file을 탭으로 열기 | GTK `DropTarget`/`gdk::FileList`로 제공 |

2026-06-16 Linux 동등성 점검에서 확정한 추가 계약:

- GTK 메뉴는 Win32 메뉴의 section 구분, 단축키 표시 라벨, 체크/비활성 상태를
  따른다. editor/tab context menu도 표시 직전 현재 상태를 갱신한 뒤 같은 명령
  집합을 보여 준다.
- Linux 기본 창 크기, find/search result/command palette/status/line-number 폭과
  theme palette는 Windows 구현의 현재 상수를 기준으로 맞춘다. search result list는
  Windows `layout_search_results`처럼 112px 영역 안에서 좌우 8px 여백과 106px
  list 높이를 유지한다. command palette는 Windows `layout_command_palette`처럼
  154px 높이 안에서 8px 여백, 24px filter, 6px 간격, 108px command list를 유지한다.
- 문서 로드처럼 platform이 편집 표면을 프로그램적으로 교체하는 흐름은 사용자
  Undo history를 만들지 않는다. 로드 직후 Undo는 이전 문서 텍스트로 돌아가지
  않아야 한다.
- modal dialog가 열려 있는 동안 Linux timer 작업은 저장 flush를
  진행하지 않는다. 사용자의 modal 결정 경계가 Windows와 같은
  작업 경계가 된다.
- GTK 파일 선택 dialog에서 Accept 결과가 local filesystem path가 아니면 취소처럼
  조용히 무시하지 않고 복구 가능한 오류로 표시한다.

2026-06-17 Linux 메뉴/상태 동등성 점검에서 확정한 추가 계약:

- Linux 시작 경로는 j3Text가 직접 수집한 startup path 계약으로만 처리한다.
  GTK `Application`에는 앱 이름만 전달해 GTK의 별도 file-open 처리와 충돌하지
  않게 한다. Windows와 같이 실행 파일명과 빈 인자는 startup path에서 제외한다.
- Linux `MainWindow` state는 GTK 창이 닫힐 때까지 보존된다. activate callback이
  끝난 뒤에도 menu/action, startup restore, timer가 같은 Rust 상태를 사용해야
  한다.
- Linux signal/timer/drop/error 경로는 programmatic UI 갱신 중 재진입하더라도
  panic하지 않는다. 이미 window state를 빌린 중첩 이벤트는 건너뛰고 다음 이벤트
  주기에서 정상 상태 갱신을 이어간다. 저장/열기 같은 modal dialog가 실행 중일 때
  들어오는 editor key event도 같은 규칙을 따른다.
- Linux File/Exit action은 GTK `close_request`가 동기 재진입하더라도 panic하지
  않아야 한다. 메뉴 action callback이 state를 빌린 상태에서 직접 close request를
  처리하지 않고, close 요청은 다음 main-loop turn에서 수행한다.
- Linux Copy/Cut은 현재 선택된 plain text를 즉시 clipboard에 반영한다. 빠른
  Copy/Cut/Paste 메뉴 연속 실행에서도 Windows Rich Edit처럼 방금 복사하거나 잘라낸
  텍스트가 붙여넣기 기준이 되어야 한다.
- Linux Paste는 GTK clipboard 삽입 완료 신호까지 app/domain 상태를 동기화한다.
  Windows Rich Edit paste처럼 메뉴 명령 뒤 저장/상태 표시가 최신 buffer 내용을
  기준으로 이어져야 한다.
- Linux Paste는 Windows Rich Edit의 `CF_UNICODETEXT` paste와 같이 clipboard의 plain
  text representation만 삽입한다. RTF/HTML 같은 rich clipboard payload가 있더라도
  문서 내용에는 plain text만 들어가야 한다.
- Linux dirty prompt는 Windows `MB_YESNOCANCEL` 흐름과 같이 Save는 blocking save 뒤
  원래 작업을 계속하고, Discard는 저장 없이 계속하며, Cancel은 원래 작업을 중단한다.
  close/open/reload/exit 계열은 모두 같은 `confirm_current_dirty` 계약을 거친다.
- Linux 확인 dialog의 버튼 라벨/순서/기본 응답은 Windows MessageBox 계약을 따른다.
  Yes/No 질문은 `Yes`, `No` 순서이며, dirty prompt는 `Yes`, `No`, `Cancel` 순서다.
  첫 버튼인 `Yes`가 기본 응답이다. Cancel 버튼이 없는 Yes/No 질문에서 GTK 창 닫기
  같은 비버튼 응답이 발생하면 Windows `MB_YESNO`처럼 `No` 흐름으로 처리한다.
- Linux message dialog는 Windows `MessageBoxW(owner, text, title, flags)`처럼
  dialog window title에 title을 두고, 본문에는 message text만 표시한다. title을
  본문 primary text로 중복 표시하지 않는다.
- Linux shortcut capture에서 이미 다른 명령이 쓰는 단축키가 입력되면 Windows
  `MB_YESNO` 흐름처럼 Yes는 기존 소유자에서 제거한 뒤 새 명령에 지정하고, No는
  기존 설정을 유지한다. 이 중복 확인은 capture뿐 아니라 Default로 단축키를 복원하는
  경로에도 동일하게 적용한다.
- Linux shortcut 설정이 바뀌면 GTK application accelerator도 즉시 갱신한다. Windows가
  editor control에서 직접 처리하는 Undo/Redo/Cut/Copy/Paste/Select All 계열은 Linux도
  `TextView` key controller 경계에서 처리하며 application accelerator에는 등록하지 않는다.
- Linux shortcut capture의 허용 key 범위는 Windows virtual-key whitelist와 맞춘다.
  top-row `0..9`, `A..Z`, `F1..F24`를 사용하고 numpad `KP_*` key는 일반 숫자
  단축키로 캡처하지 않는다.
- Linux shortcut capture 중 잘못된 단일 문자 key를 누르면 Windows처럼 capture
  dialog를 owner로 둔 `Shortcut` warning dialog를 표시한다. 경고가 main window에
  붙어 capture dialog 뒤에 가려지거나 포커스 순서가 달라지지 않아야 한다.
- Linux editor shortcut repeat guard는 Windows `WM_KEYDOWN` repeat bit와 같은 효과만
  내야 한다. modifier를 먼저 떼더라도 눌린 주 key가 release되면 guard를 해제해 이후
  같은 shortcut이 다시 실행되어야 한다.
- Linux Recent 메뉴는 app 상태가 이미 정규화되어 있더라도 Windows `MainMenuBuilder`처럼
  메뉴 생성 단계에서 다시 최대 10개만 표시한다.
- Linux Marks는 표시 한계를 넘는 문서에서 Windows와 같은 제한 안내 문구를 정보
  dialog로 보여 주고 normal view를 유지한다. 큰 문서 로드 중 자동으로 Marks가 꺼진
  경우에는 Windows처럼 현재 세션 상태만 보정하고 사용자 설정 파일에는 저장하지
  않는다.
- 메뉴 설정 변경은 저장 flush 뒤 다음 실행에서 앱 설정뿐 아니라 GTK stateful
  action의 체크 상태로도 복원된다.
- Linux 메뉴 설정 변경은 Windows처럼 명령 결과와 체크 상태를 즉시 반영하되, GTK
  `PopoverMenuBar` 모델 재구성은 활성 메뉴 action callback이 끝난 뒤 idle 시점에
  수행한다. 메뉴가 닫히는 중 모델을 교체해 발생하는 native crash를 피하기 위함이다.
- Linux Command Palette 토글은 Windows `View > Commands`처럼 표시 상태와 메뉴 체크
  상태를 같은 명령 처리 안에서 함께 갱신한다.
- Linux editor context menu는 Windows `SetFocus(self.edit)`처럼 메뉴 표시 전 편집
  surface를 현재 focus 대상으로 지정하고, popover가 닫힌 뒤에도 idle 시점에 같은
  focus를 되돌린다.
- Open/Save As/Font/Reopen Encoding/Change Encoding dialog의 accept 결과는
  platform 내부에서 path/font/encoding 값으로 변환된 뒤 Windows와 같은
  app/domain/infra 경계에 적용된다.
- Linux Font dialog accept 결과의 point size는 Windows `ChooseFontW` 결과 처리처럼
  가장 가까운 정수 pt로 반올림하고 4~288pt 범위로 제한한 뒤 설정에 저장한다.
- Linux encoding selection dialog는 Windows처럼 현재 입력의 validation status를
  계속 표시한다. 잘못된 manual input에서 OK/Enter를 눌러도 dialog를 닫거나 문서
  상태를 바꾸지 않고, Windows와 같은 `Encoding` 경고 문구를 표시한 뒤 입력을
  계속 수정할 수 있어야 한다.
- Linux Save As file dialog는 Windows `GetSaveFileNameW`처럼 처음 열 때 filename
  입력을 비워 둔다. 현재 문서가 file-backed여도 기존 파일명을 미리 채우지 않는다.
  기존 파일을 선택하면 Windows `OFN_OVERWRITEPROMPT`처럼 `Confirm Save As`
  Yes/No 확인을 표시하고, No는 encoding 선택과 저장을 시작하지 않은 채 원래 문서를
  유지한다. GTK4 `FileChooserNative`에는 이 Windows flag에 대응하는 공개 설정이
  없으므로 Linux backend는 파일 선택 결과의 target metadata를 캡처한 직후 같은 확인
  경계를 직접 적용한다.
- Linux 저장 교체는 Windows의 기존 파일 replace 안전 기준처럼, 기존 target을
  보존해야 하는 저장에서는 target이 교체 시점에도 존재할 때만 성공해야 한다. Linux는
  `renameat2(RENAME_EXCHANGE)`로 임시 파일과 기존 target을 원자적으로 교환해 target
  삭제 race에서 새 파일을 생성하지 않는다.
- Linux 저장 target 검사는 Windows의 외부 변경 보호 결과를 우선한다. Linux `ctime`은
  빠른 같은 크기 외부 변경에서 동일하게 보일 수 있으므로 Windows `ChangeTime`처럼
  unchanged 확정 marker로 쓰지 않고, 저장 시점 content fingerprint guard를 유지한다.
- Linux 상태바 파트 폭과 창 제목 포맷은 Windows status control 계약을 따른다.
  상태바는 Windows `SB_SETPARTS` 경계와 같은 폭으로 표시하고, 문서 제목이 비어도
  창 제목은 `j3Text - j3Text` 형식을 유지한다. 상태바 column 값은 Windows Rich
  Edit처럼 현재 줄 시작의 UTF-16 offset 대비 selection start UTF-16 offset으로
  계산한다.
- Linux find bar의 label/button 폭과 control 높이는 Windows `layout_find_bar`
  상수와 맞춘다. Entry는 Windows와 같은 최소 120px 폭을 유지하고 남는 공간을
  나누어 가진다.
- Linux tab tooltip은 Windows와 같이 file-backed 문서의 전체 경로를 보여 주고,
  긴 경로는 80자 기준으로 path separator를 우선해 여러 줄로 감싼다.
- Linux line-number view는 Windows처럼 현재 첫 visible line부터 최대 200줄만
  줄 번호 텍스트를 만들고, 같은 visible snapshot이면 재생성을 생략한다.
- Linux Open in New Window 실패 메시지는 Windows와 같은 사용자 동작명
  `open in new window`와 내부 context를 사용한다.

상세 점검 기록과 메뉴별 검증 표는 `docs/linux-menu-parity-report.md`에 둔다.

남은 의도적 차이:

- Windows non-client dark chrome과 native menu dark rendering은 Win32 전용 경계다.
  Linux는 GTK theme/CSS 경계 안에서 같은 문서/명령 상태를 표시한다.
- Windows의 file change marker는 Windows metadata 전용 최적화다. Linux는 modified
  time과 byte length, 필요 시 content snapshot 비교로 같은 보수적 저장 안전 규칙을
  지킨다.

## 완료 기준

- Windows에서 주 편집 컨트롤이 `RICHEDIT50W`로 생성되고, 문서 텍스트를 넣기
  전에 plain text mode로 고정된다.
- `{\\rtf1 ...}`처럼 RTF처럼 보이는 문자열도 리터럴 plain text로 열고,
  편집하고, 저장하고, 검색한다.
- RTF stream flag, rich formatting command, embedded object command가 app/domain
  계약이나 사용자 기능으로 노출되지 않는다.
- 새 문서는 UTF-8, CRLF, clean, writable 상태로 시작한다.
- 열기/저장은 선택된 `TextEncoding`과 `LineEnding`을 보존하거나 사용자의 명시적
  변경에 따라 갱신한다.
- 변경 감지는 Rich Edit modify flag가 아니라 app/domain의 clean baseline
  비교로 판정한다.
- Undo/Redo, Cut/Copy/Paste, Select All은 현재 주 편집 표면의 plain text 명령으로
  동작한다. Win32/Rich Edit 메시지 매핑은 platform 어댑터가 감추고, 텍스트
  변경 명령 뒤에는 app/domain으로 즉시 동기화된다.
- 100 MB 이상 파일은 읽기 전에 사용자 확인을 요구하고, 확인 후에는 read-only로
  연다. 확인된 큰 파일도 128 MB를 넘으면 현재 in-memory 모델의 안전 상한을 넘어
  열지 않는다.
- dirty 문서를 닫거나 다른 파일로 교체하거나 종료할 때 Save/Discard/Cancel
  결정을 먼저 받는다.
- read-only 문서, save 진행 중 문서, visible whitespace 표시 중인 표면은 일반
  편집과 ordinary Save를 막는다. Save As는 쓰기 가능한 새 대상이면 read-only를
  해제한 새 저장 기준을 만든다.
- Word Wrap은 사용자 설정으로 저장되는 view 상태다. 토글해도 문서 본문,
  dirty state, search range, 저장 bytes는 바뀌지 않는다.
- 변경 뒤 최소 검증은 `cargo check`를 포함한다. Rich Edit 경계 변경은 가능하면
  platform unit test와 plain-text/large-file regression test까지 확인한다.

## 텍스트 인코딩 정책

- 지원 인코딩은 `TextEncoding`의 19개 값이다: UTF-8, UTF-8 BOM, UTF-16 LE,
  UTF-16 BE, EUC-KR, CP949, Shift-JIS, GB18030, Big5, ISO-8859-1,
  Windows-1250, Windows-1251, Windows-1252, Windows-1253, Windows-1254,
  Windows-1255, Windows-1256, Windows-1257, Windows-874.
- 새 문서와 빈 파일의 기본 인코딩은 UTF-8이다.
- 자동 열기는 BOM을 먼저 본다. 그 다음 BOM 없는 UTF-16 NUL 패턴, UTF-8,
  한국어/일본어/중국어 코드 페이지의 round-trip 확인, ISO-8859-1 fallback
  순서로 판단한다.
- Reopen Encoding은 사용자가 선택한 인코딩으로 자동 감지를 우회한다.
  강제 UTF-8로 UTF-8 BOM bytes를 열면 U+FEFF는 본문 문자로 남고,
  `TextEncoding::Utf8Bom`일 때만 BOM marker를 제거한다.
- Save As는 대상 경로 선택 뒤 출력 인코딩을 묻고, 성공한 뒤 문서 인코딩을 그
  값으로 바꾼다.
- Change Encoding은 저장하지 않고 현재 문서의 선택 인코딩만 바꾼다. writable
  문서에서 값이 바뀌면 다음 저장 bytes가 달라지므로 dirty가 된다.
- legacy/code page 인코딩은 저장 시작 전에 현재 text가 표현 가능한지 확인한다.
  표현할 수 없으면 쓰기를 시작하지 않고 사용자에게 경고한다.
- 디코딩된 text에 NUL 문자가 있으면 안전하게 열 수 없는 plain text로 보고
  거부한다.
- UTF-8 BOM은 `TextEncoding::Utf8Bom`으로 감지되거나 사용자가 그 인코딩을
  강제했을 때만 파일 marker로 처리하고 본문에서 제거한다. 사용자가
  `TextEncoding::Utf8`을 강제하면 BOM bytes는 유효한 UTF-8 본문 문자
  U+FEFF로 남는다.
- 자동 열기에서 BOM 없는 bytes가 UTF-8 디코딩에 실패하면 즉시 실패하지 않고
  지원하는 legacy/code page round-trip 감지를 시도한 뒤 ISO-8859-1 fallback을
  사용한다. 이때 app/domain에는 선택된 `TextEncoding`과 plain `String`만
  전달한다.
- 사용자가 UTF-8 또는 UTF-8 BOM을 명시적으로 선택한 reopen 흐름에서는 UTF-8
  디코딩 실패를 다른 인코딩으로 fallback하지 않는다. UTF-8 BOM이 있는 파일의
  BOM 뒤 payload가 유효한 UTF-8이 아니어도 자동 열기는 실패한다.
- 저장은 `TextEncoding::Utf8`에는 BOM을 쓰지 않고, `TextEncoding::Utf8Bom`,
  `TextEncoding::Utf16Le`, `TextEncoding::Utf16Be`에는 각 인코딩의 BOM을 쓴다.
  다른 인코딩은 BOM을 추가하지 않는다.

## 줄바꿈 정책

- `LineEnding`은 저장 시 사용할 영속화 정책이며 CRLF, LF, CR만 지원한다.
- 새 문서의 기본 줄바꿈은 CRLF다.
- 파일을 열 때 본문에서 CRLF, LF, CR 개수를 세어 가장 많은 줄바꿈을 선택한다.
  동률이면 CRLF, 그 다음 LF, 그 다음 CR 순서로 선택한다. 줄바꿈이 없는 문서는
  기본값인 CRLF로 취급된다.
- Rich Edit은 내부 문단 구분을 단일 CR로 다룰 수 있으므로 platform은 전체
  본문을 app/domain으로 꺼낼 때 `GT_USECRLF`/`GTL_USECRLF`를 사용해 CRLF 기반
  plain text로 동기화한다.
- 저장은 `FileDocumentIo`가 현재 `Document.line_ending`에 맞춰 CRLF/LF/CR로
  normalize한 bytes를 쓴다.
- 사용자가 line ending을 바꾸면 writable 문서는 dirty가 된다. 이 변경은 본문
  표시를 즉시 대량 치환하기 위한 명령이 아니라 다음 저장 결과의 계약이다.
- 줄바꿈 변환 알고리즘은 domain의 `LineEnding` helper 한 곳에 둔다.
  `LineEnding::normalize_text`는 테스트와 작은 문자열 비교용이고, infra 저장
  경계는 같은 helper를 streaming 방식으로 호출해 큰 문서 저장 중 별도 normalized
  `String`을 만들지 않는다.

## 파일 I/O 경계와 오류 메시지 정책

- 파일 bytes 읽기, metadata 확인, 인코딩 감지/변환, 줄바꿈 정규화 후 byte 쓰기,
  임시 파일 생성/flush/sync/replace는 infra 책임이다. 큰 파일 로드는 bytes를
  읽는 동안 snapshot fingerprint를 함께 갱신해 별도 전체 byte 순회를 피한다.
  app/domain은 `LoadedDocument.content: String`, `TextEncoding`, `LineEnding`,
  `FileSnapshot`, 의미 있는 `AppError`만 받는다.
- app/domain의 내부 문서 표현은 `Document.content: Arc<str>`인 plain Unicode
  text다. BOM, code page bytes, OS file handle, temporary path는 이 경계를
  넘어오지 않는다.
- 복구 가능한 파일/인코딩 실패는 `Result`로 전달한다. 런타임 파일 I/O 경로에서
  `panic!`, `unwrap()`, `expect()`로 사용자 파일 오류를 처리하지 않는다.
- `AppError::Io`는 내부 원인(`io::Error`, 작업 context, path)과 사용자 표시
  메시지를 분리한다. 사용자 메시지는 permission denied, file-in-use, read-only,
  not found, 기타 I/O를 구분한다.
- 저장 중 임시 파일 생성/쓰기/flush/sync/replace에서 실패해도 사용자 메시지는
  내부 임시 파일명이 아니라 저장 대상 파일과 `save file` 행동을 기준으로 표시한다.
  내부 `Display`/debug 정보에는 임시 파일 context와 원인을 남긴다.
- `AppError::Encoding`은 내부 원인 detail과 사용자 표시 메시지 범주를 분리한다.
  decode 실패, encode 실패, NUL 포함 같은 unsafe text 실패는 서로 다른 범주로
  보존하고, UI에는 선택 가능한 다음 행동을 이해할 수 있는 메시지만 보여 준다.
- `Display`/debug 출력은 개발자와 로그가 원인을 추적할 수 있도록 내부 context와
  detail을 남기고, `user_message()`는 사용자에게 과도한 내부 구현명을 노출하지
  않는 표시용 문구를 반환한다.
- 사용자에게 보이는 메뉴, 상태바, 대화상자, 오류 문구는 짧고 쉬운 단어를
  우선한다. 내부 로그와 개발자용 `Display` 문구는 원인 추적을 위해 더 자세할 수
  있지만, UI 문구는 사용자가 다음 행동을 바로 고를 수 있게 한두 문장으로 제한한다.

## 변경 감지 계약

- dirty state의 source of truth는 app/domain이다. `Document`는 clean baseline의
  content, encoding, line ending과 현재 상태를 비교하고, backing file missing
  여부까지 반영한다.
- Rich Edit 자체 modify flag는 dirty 판정 기준이 아니다. Undo로 보이는 text가
  저장 기준과 같아져도 Rich Edit modify flag는 남을 수 있기 때문이다.
- `EN_CHANGE`는 문서를 dirty로 표시하고 `edit_content_pending_sync`를 세운다.
  매 입력마다 전체 buffer를 복사하지 않는다.
- 이미 dirty인 문서에서 반복 `EN_CHANGE`가 들어와도 dirty 표시 전환으로 인한
  탭 제목 재구성은 다시 수행하지 않는다.
- save, search, tab switch, close/open/exit dirty prompt처럼
  최신 text가 필요한 유스케이스에서만 `EM_GETTEXTLENGTHEX`와 `EM_GETTEXTEX`로
  주 편집 표면을 app/domain에 동기화한다.
- Undo, Redo, Cut, Paste처럼 platform이 직접 실행한 text-mutating 명령은 명령
  직후 동기화한다. 그래서 Undo가 저장 기준으로 돌아오면 tab dirty 표시도 바로
  clean으로 돌아갈 수 있다.
- 외부 파일 변경은 background polling으로 미리 감지하지 않는다. Save 또는 Save As
  시점에 마지막 file snapshot과 저장 target의 metadata/content fingerprint를
  비교해 충돌을 판정한다.

## Undo/Redo 지원 범위

- Undo/Redo는 현재 활성 Rich Edit text surface의 undo stack에 맡긴다. app/domain은
  별도 undo history를 저장하거나 세션에 보존하지 않는다.
- Undo, Redo, Cut, Copy, Paste, Select All은 `EditorTextSurface`의 plain text
  명령으로 실행한다. 현재 Win32 구현은 내부에서 Rich Edit/Win32 메시지로
  변환하지만, app/domain은 이 메시지 이름이나 Rich Edit undo stack 타입을
  알지 않는다.
- Undo/Redo는 plain text 편집 결과만 대상으로 한다. 서식 변경, RTF 속성,
  embedded object 조작은 지원 범위가 아니다.
- read-only 문서, save 진행 중 문서, visible whitespace 표시 중인 표면에서는
  text-mutating 명령을 막는다.
- tab 전환, 문서 reload, 앱 재시작 뒤의 undo history 보존은 현재 계약에 포함하지
  않는다. 이 상태들은 `Document.content`와 view state를 다시 로드하는 경계다.

## 큰 파일 처리 기준과 제한

- `LARGE_FILE_THRESHOLD_BYTES`는 100 MB다. 이 크기 이상 파일은 읽기 전에
  사용자에게 확인을 받아야 한다.
- 사용자가 100 MB 이상 파일 열기를 취소하면 파일 bytes를 읽지 않는다.
- 사용자가 계속 진행하면 문서는 `ReadOnlyReason::LargeFile`로 열린다. 현재
  모델은 단일 in-memory document buffer와 Win32 edit control 동기화를 사용하므로
  큰 파일 편집과 ordinary Save는 이번 계약에서 막는다.
- 일반 load 상한인 `MAX_DOCUMENT_LOAD_BYTES`는 100 MB다.
- 확인된 큰 파일의 안전 read 상한은 infra의
  `MAX_CONFIRMED_LARGE_FILE_LOAD_BYTES`인 128 MB다. 이 값을 넘는 파일은 확인을
  받아도 열지 않고 file-too-large 오류를 반환한다.
- 큰 파일 검색/저장 관련 흐름은 가능한 한 bounded chunk와 cached metrics를
  사용한다. 다만 true virtualized multi-hundred-MB editing은 현재 완료 기준이
  아니며 future paged document model의 범위다.

## 사용자 상태 규칙

- dirty 문서를 닫기, 다른 파일 열기, dirty 현재 탭 교체, 앱 종료 전에
  Save/Discard/Cancel을 묻는다. Save를 선택하면 완료 결과가 필요한 dirty prompt
  경로에서는 blocking save를 허용한다.
- 직접 Save/Save As 명령은 document snapshot을 잡아 background save job으로
  실행한다. 저장 중인 현재 문서는 read-only로 잠가 완료된 save가 더 최신 편집을
  clean으로 덮지 않게 한다.
- 저장 target이 외부 변경되어 Save가 실패하면 단순 오류 확인이 아니라 `Reload`,
  `Save As`, `Cancel` 선택을 제공한다. `Reload`는 해당 탭을 선택하고 디스크
  내용을 다시 읽어 현재 메모리 내용을 버리며, `Save As`는 해당 탭을 선택한 뒤
  기존 Save As 흐름으로 현재 메모리 내용을 다른 target에 저장한다. `Cancel`은
  문서 상태를 그대로 둔다.
- 현재 UX 상태의 source of truth는 app 계층의 `CurrentEditorStatus`다. 이 상태는
  현재 문서의 dirty/clean, ordinary Save 가능 여부, Save As 가능 여부, 현재 파일
  경로, read-only 여부, Word Wrap 여부, Undo/Redo 가능 여부, 현재 줄/열을 합성한다.
  메뉴 활성화와 상태바 표시는 이 상태를 읽는다.
- file-system read-only attribute가 있는 파일은 `ReadOnlyReason::FileAttribute`로
  열린다. read-only 문서는 일반 편집과 ordinary Save가 막히며, Save As가 성공한
  writable 대상은 새 clean baseline이 된다.
- visible whitespace는 표시 전용 상태이며 본문, dirty state, search range, 저장
  결과를 바꾸지 않는다. 표시 중에는 Rich Edit 표면을 read-only로 두고 ordinary
  Save를 막는다. Save As는 app에 이미 동기화된 실제 document text를 저장한다.
- visible whitespace는 전체 표시 문자열을 별도로 만들어야 하므로 일반 load 상한의
  1/4을 넘는 문서에서는 켜지 않는다. 이 경우 본문은 normal text view로 유지하고
  전체 렌더링 문자열을 만들지 않는다.
- Word Wrap은 `EditorSettings.word_wrap`에 저장되는 view preference다. 켜면
  horizontal scroll 스타일을 제거하고 Rich Edit target device 폭을 wrap 모드로
  설정한다. 꺼져 있으면 `ES_AUTOHSCROLL | WS_HSCROLL`을 사용한다.
- 상태바는 현재 줄/열, 전체 문자 수, 선택 문자 수, encoding, line ending, Word
  Wrap, 저장 가능 상태, UX 상태와 현재 파일 경로를 표시한다. 선택이 없는 caret
  이동에서는 선택 문자 계산을 위해 전체 document offset mapping을 수행하지
  않는다. 저장 가능 상태는 `Can Save`, `Save As`, `No Save` 중
  하나다.

## Rich Edit 기반 plain text editor 구현 구조

### 현재 Win32 텍스트 표면 구조

- 엔트리포인트: `src/main.rs`는 `platform::run()`만 호출하고,
  `src/platform/win32.rs`가 Win32 윈도우 클래스, 메시지 루프, 컨트롤,
  메뉴, 다이얼로그, 타이머, 드래그 앤 드롭, 에디터 뷰 상태를 소유한다.
- 주 편집 컨트롤: `MainWindow::create_controls`는 `self.edit`를
  `create_editor_text_surface`를 통해 `RICHEDIT50W` 클래스로 생성한다.
  `platform::run()`은 메인 윈도우 생성 전에 `Msftedit.dll`을 로드하고,
  컨트롤 생성 helper도 같은 초기화를 idempotent하게 확인한다. 스타일은
  `ES_MULTILINE`, `ES_AUTOVSCROLL`, `ES_NOHIDESEL`, `WS_VSCROLL`이며, word
  wrap이 꺼져 있으면 `ES_AUTOHSCROLL | WS_HSCROLL`을 추가한다.
- 편집 표면 어댑터: `src/platform/win32.rs`의 `EditorTextSurface`가
  `HWND`, `EM_*`, `WM_*`, `CHARFORMAT` 계열 구조체와 Rich Edit text get/set
  세부사항을 감싼다. `MainWindow`의 유스케이스 코드는 가능한 한
  `get_text`, `set_text`, `set_readonly`, `select_all`, `undo`, `redo`,
  `can_undo`, `can_redo` 같은 plain text editor 동작으로만 호출한다.
- 보조 Edit Control: line numbers, find input, replace input, command palette
  filter도 `"EDIT"`를 쓴다. Rich Edit 전환 대상은 주 텍스트 표면인
  `MainWindow.edit`만으로 한정한다. 보조 컨트롤은 별도 필요가 확인되기
  전까지 기존 Edit Control로 유지한다.
- 텍스트 로드 경로: infra가 파일 bytes를 `LoadedDocument`로 디코딩하고,
  app/domain이 `Document.content`에 plain text를 보관한 뒤, platform이
  `load_current_document_into_edit`에서 Rich Edit plain-text helper를 통해
  `EM_SETTEXTEX`/Unicode/`ST_PLAINTEXTONLY`로 컨트롤에 싣는다.
- 텍스트 편집 경로: Rich Edit 알림은 platform에서 `EditorSurfaceEvent` 같은
  의미 있는 editor 상태 변경으로 변환된다. text-changed 이벤트는 도메인 문서를
  dirty로 표시하고 `edit_content_pending_sync`만 세운다. 전체 텍스트 복사는 save,
  search, tab switch, close prompt처럼 정확한 최신 본문이 필요한
  유스케이스에서만 `EM_GETTEXTLENGTHEX`와 `EM_GETTEXTEX`로 수행한다.
- 이벤트/명령 경로: 메뉴, 단축키, find bar Enter/Esc, command palette는
  `WM_COMMAND`로 모여 platform/app handler를 탄다. undo, redo, cut, copy,
  select all은 Win32/Rich Edit 메시지를 주 편집 컨트롤에 전달한다. paste는
  Rich Edit의 "best available format" 선택 경로를 쓰지 않고
  `EM_PASTESPECIAL`과 `CF_UNICODETEXT`로 plain Unicode text만 요청한다.
  변경 감지, 선택 변경, 스크롤, 포커스 변경은 app의 `EditorSurfaceState` 갱신으로
  모인 뒤 status, line number, menu state를 갱신한다.
- 저장/로드 경로: `FileDocumentIo`가 metadata, decode, encode, line ending
  detection, encoding validation, atomic replace save를 맡는다. platform은
  path/encoding dialog를 담당하고, app/domain은 document state, dirty state,
  snapshot, recent-file projection을 결정한다.

### 핵심 용어

- `Document`: Unicode plain text 본문과 file identity, encoding, line ending,
  dirty/read-only state, file snapshot metadata를 가진 문서.
- EditorTextSurface: 활성 `Document`를 보여주고 입력받는 Win32
  텍스트 편집 표면. 현재 주 편집 표면은 Rich Edit Control 기반이지만
  app/domain에는 plain text만 노출한다.
- RichEditPlainTextMode: Rich Edit Control을 RTF/서식 문서 편집기가 아니라
  plain text 편집기로만 사용한다는 platform 불변식.
- TextSync: 현재 `EditorTextSurface`의 plain text를 필요한 시점에만
  `Document.content`로 복사하는 app/platform 경계 작업.
- TextPersistence: 선택된 encoding과 line ending에 따라 file bytes와 plain
  document text 사이를 변환하는 infra 소유 저장/로드 동작.

### Edit Control에서 Rich Edit Control로 바꾸는 목적

- 주 편집 컨트롤의 Win32 구현만 Rich Edit으로 바꿔 plain text 편집 표면의
  한계를 줄이되, 제품 도메인은 계속 plain text editor로 유지한다.
- 기존 app/domain 동작인 tabs, dirty prompts, search/replace, line endings,
  encodings, read-only policy, recent files, atomic save 의미를
  plain-text 기준으로 보존한다.
- 변경을 platform 경계 안에 가둔다. `domain`, `app`, `infra`는 Rich Edit
  handle, RTF 개념, formatting API에 의존하지 않는다.

### Rich Edit Plain Text 고정 규칙

- Rich Edit은 주 편집 컨트롤 생성 전에 platform 코드에서 초기화한다. 선택한
  Rich Edit class가 요구하는 라이브러리 로드도 이 경계에서 처리한다.
  현재 구현은 `Msftedit.dll`을 로드한 뒤 `RICHEDIT50W` 클래스로만 주 편집
  컨트롤을 생성한다.
- 주 편집 Rich Edit Control은 생성 직후, 문서 텍스트를 넣기 전에 반드시
  plain text mode로 설정한다. 설정 실패 시 rich-text mode로 계속 실행하지
  말고 `AppError`로 컨트롤 생성 또는 시작을 실패시킨다.
- `EM_SETTEXTMODE`는 컨트롤에 텍스트가 있으면 실패하므로
  `create_editor_text_surface`는 빈 텍스트로 컨트롤을 만든 뒤 텍스트 길이가
  0인 것을 확인하고 `EM_SETTEXTMODE`/`TM_PLAINTEXT`를 보낸다. 문서 본문은 이
  절차가 끝난 뒤 `load_current_document_into_edit`에서만 싣는다.
- 에디터는 RTF를 document content로 저장, 스트리밍, 노출하지 않는다. save,
  search에 쓰는 모든 컨트롤 텍스트 추출은 plain Unicode text만
  반환해야 한다.
- formatting operation, rich text selection attribute, embedded object, OLE,
  RTF stream flag는 도메인 밖이다. 구현 단계에서 app/domain 계약에 추가하지
  않는다.
- Rich Edit event mask는 plain text editor 상태 갱신에 필요한 알림만 켠다.
  `EM_SETEVENTMASK`로 change, selection change, scroll 알림을 요청하고,
  formatting/RTF/OLE 관련 알림과 명령은 노출하지 않는다.
- Rich Edit 생성 또는 plain text mode 설정이 `WM_CREATE` 안에서 실패해도
  바깥 `CreateWindowExW`의 일반 실패로 덮어쓰지 않는다. platform은 생성
  컨텍스트에 실제 `AppError`를 보존하고, `run()`은 보존된 원인을 우선
  반환한다.
- read-only, word wrap, selection, first visible line, tab stops, undo/redo/
  cut/copy/paste/select-all, find/replace, status metrics, line-number refresh는
  Rich Edit 메시지 동작으로 다시 검증한다. Edit Control 메시지와 Rich Edit
  메시지가 항상 동일하게 동작한다고 가정하지 않는다.

### Rich Edit 전환 회귀 검증 범위

자동화 검증은 UI 이벤트 전체를 대신하지 않고, Rich Edit 전환에서 깨지기 쉬운
plain text 불변식과 app/infra 저장 흐름을 우선 확인한다.

- platform 단위 테스트는 숨은 `RICHEDIT50W` 컨트롤을 생성해
  `EM_SETTEXTEX`/`EM_GETTEXTEX` plain text 왕복, RTF처럼 보이는 문자열의
  리터럴 보존, 보조 평면 Unicode 문자 보존, 선택 영역과 전체 선택 영역의
  plain text 치환, Undo/Redo 복원을 검증한다.
- platform 단위 테스트는 `Msftedit.dll`이 컨트롤 생성 전에 로드되었는지,
  텍스트가 들어간 뒤 `EM_SETTEXTMODE`를 허용하지 않는지, child
  `CreateWindowExW` 실패 코드와 startup 생성 내부 오류가 보존되는지도
  검증한다.
- app/infra 통합 테스트는 새 문서의 기본 UTF-8/CRLF/clean 상태, 한글/영문/
  숫자/기호가 섞인 본문 저장과 재열기, CRLF 줄바꿈 보존, 빈 문서 저장과
  재열기, Rich Edit 경계에서 CRLF로 동기화된 본문을 LF 저장 정책으로 다시
  저장하는 흐름, 청크 저장 경로를 타는 큰 문서의 LF 줄바꿈 보존을 검증한다.
- 기존 app 테스트는 delayed TextSync가 edit notification 단계에서 본문을
  즉시 교체하지 않고 dirty 상태만 표시하는지 확인한다.

Win32 UI 수동 검증 체크리스트:

- 상세 수동 회귀 기준은 `docs/rich-edit-plain-text-regression-checklist.md`에
  별도 문서로 고정한다.
- 새 문서를 만들고 한글, 영문, 숫자, 기호를 직접 입력했을 때 탭 제목의 dirty
  표시와 상태 표시가 갱신되는지 확인한다.
- CRLF, LF, CR 줄바꿈을 가진 파일을 열고 저장한 뒤 선택한 line ending 정책이
  파일 bytes에 반영되는지 확인한다.
- 서식이 있는 외부 앱과 브라우저에서 붙여넣기했을 때 서식/RTF가 삽입되지
  않고 Unicode plain text만 들어오는지 확인한다.
- 입력, 붙여넣기, 선택 영역 치환 뒤 Undo/Redo가 본문과 dirty 표시를 일관되게
  갱신하는지 확인한다.
- 빈 파일과 100 MB 경계 파일을 열 때 각각 정상 편집 상태와 large-file
  확인/read-only 정책이 유지되는지 확인한다.
- 저장, 다른 이름으로 저장, 저장 실패 후 원본 보존, 재열기 후 최근 파일 표시가
  기존 plain text editor 동작과 같은지 확인한다.

### Rich Edit Plain Text 입출력과 줄바꿈 정책

- 주 편집 `EditorTextSurface`의 본문 설정은 `EM_SETTEXTEX`를 사용한다.
  `codepage`는 Unicode code page인 1200으로 고정하고, flags는 UTF-16 입력과
  plain-text-only 삽입을 명시한다. 따라서 본문이 RTF 시작 문자열처럼 보여도
  document content 경로에서는 서식 문서로 해석하지 않는다.
- 본문 읽기는 `EM_GETTEXTLENGTHEX`로 Unicode 문자 수를 구한 뒤
  `EM_GETTEXTEX`로 수행한다. 큰 텍스트에서 `WM_GETTEXTLENGTH`/
  `WM_GETTEXT`의 edit-control 호환 경로에 기대지 않고, Rich Edit이 계산한
  버퍼 크기와 Unicode 변환 경로를 사용한다.
- Rich Edit은 문단 구분을 내부적으로 단일 CR 단위로 다룬다. platform은
  전체 본문을 꺼낼 때 `GT_USECRLF`/`GTL_USECRLF`를 사용해 app/domain에 CRLF
  plain text를 전달한다. `Document.line_ending`은 저장 시 사용할 영속화
  정책이며, `FileDocumentIo`가 최종 file bytes를 쓸 때 CRLF/LF/CR로
  normalize한다.
- selection, status, find result navigation처럼 UTF-16 offset을 주고받는
  경로는 Rich Edit의 단일-CR offset과 app text offset이 다를 수 있다.
  변환은 `src/platform/win32.rs`의 Rich Edit text-surface helper에 모아 두고,
  app/domain에는 Win32 offset 규칙을 노출하지 않는다.
- 보조 `"EDIT"` 컨트롤인 find input, replace input, command filter,
  line-number view와 window title/status text는 기존 generic window text
  helper를 계속 사용한다. Rich Edit 전용 text helper는 주 편집 본문에만
  적용한다.

### UI/Infra와 App/Domain 경계

- UI/platform: Win32 handle, Rich Edit initialization, control class name,
  control message, notification, selection range, scroll position, focus,
  layout, theme/color 처리, UTF-16 control offset과 Rust string range 변환을
  소유한다.
- app: document/tab 유스케이스 흐름, command dispatch decision, dirty prompt
  순서, recent-file state, search result state, deferred text-sync 조합을
  소유한다.
- domain: plain text document 규칙, encoding/line-ending 값, dirty/read-only
  state, search range calculation, save policy planning, editor settings를
  소유한다. Rich Edit 용어를 포함하지 않는다.
- infra: file byte I/O, metadata, encoding detection/conversion, save 시 line
  ending normalization, settings/recent-file persistence, atomic
  replacement를 소유한다. control-specific data가 아니라 plain text만
  주고받는다.

### 텍스트 입출력 정책

- canonical in-memory document content는 계속 `Document.content`의 plain Rust
  `str`이다. Rich Edit Control은 동기화 전의 view/input surface이지, 동기화
  뒤의 source of truth가 아니다.
- 로드는 기존 정책을 유지한다: file bytes -> infra decode ->
  `LoadedDocument.content` -> app `Document` -> platform이 editor surface에
  plain text로 반영한다.
- 저장도 기존 정책을 유지한다: platform이 editor surface의 pending plain
  text를 동기화 -> app/domain snapshot -> infra encoding 및 line-ending
  normalization -> atomic replace.
- deferred synchronization 불변식은 유지한다. edit notification은 dirty만
  표시하고, 전체 buffer copy는 정확한 최신 본문이 필요한 유스케이스까지
  미룬다.
- 모든 주 편집 platform text extraction/insertion 경로는 Rich Edit
  plain-text API 또는 plain-text flag만 사용한다. RTF stream mode 같은 rich
  text flag는 document content 경로에서 금지한다.
- NUL 문자 거부, UTF-16 offset 변환, read-only enforcement, background save
  중 read-only locking, large-file/read-only policy는 컨트롤 교체 후에도 같은
  app/domain 용어로 관찰되어야 한다.
- 이번 구현은 주 편집 `EditorTextSurface`에 대한 작은 platform helper를
  만들고 backing control을 Rich Edit으로 바꾼다. `find_edit`,
  `replace_edit`, command filter, line-number control은 기존 Edit Control로
  유지하며, 테스트나 Win32 동작상 필요한 경우에만 별도 조정한다.

## Requirements

j3Text is a Windows and Linux tab-based plain text editor. This step keeps the
existing new/open/save/save as, dirty prompts, find/replace, line numbers, status
bar, and menu flows, and adds the minimum durable foundation for daily file
editing on both platform backends.

- Persist user settings next to the executable as `<executable-name>.toml`.
  Load only the current TOML settings format; do not migrate older settings
  formats or removed keys. Persist recent files per OS user.
- Store, show, and reopen recent files.
- Do not restore ordinary open tabs on startup. When launched without file path
  arguments, start with a new untitled document.
- When launched with file path arguments, open those paths as startup tabs. If a
  startup path does not exist, ask whether to create a new file for that path.
- Detect CRLF, LF, and CR line endings, show the current value, convert it on
  request, and preserve the selected format on save.
- Support UTF-8, UTF-8 BOM, UTF-16 LE, UTF-16 BE, EUC-KR, CP949, Shift-JIS,
  GB18030, Big5, ISO-8859-1, Windows-1250, Windows-1251, Windows-1252,
  Windows-1253, Windows-1254, Windows-1255, Windows-1256, Windows-1257, and
  Windows-874 for display and save-as-encoding flows.
- Let Save As choose the output encoding for the new target path.
- Let the user reopen a file with a different encoding when auto detection is
  wrong. The Text menu exposes one Reopen Encoding command and the
  selected encoding is chosen in a modal dialog. The dialog offers a drop-down
  list of supported encodings and also accepts a manually typed encoding name.
  Invalid manual input must keep validation text and OK/Cancel/window-close
  controls visible so the dialog can be corrected or dismissed without changing
  the document.
- Let the user convert the current document to another output encoding from the
  Text menu. Change Encoding uses the same encoding-selection dialog,
  validates that the current text can be represented by the target encoding, and
  marks writable documents dirty when the selected encoding changes.
- Do not expose Open As or Save Encoding commands under the Text menu. Open
  uses automatic detection, Save As still prompts for the target encoding, and
  Reopen Encoding is the explicit correction flow.
- Warn before saving if text cannot be represented by the selected encoding.
  This preflight applies before both blocking dirty-prompt saves and direct
  background Save/Save As workers are started.
- Warn before opening files at or above 100 MB, and open them read-only when the
  user continues.
- Persist font name, font size, tab size, word wrap default, and
  theme preferences. Theme choices are System, Light, Classic Dark, Sepia Teal,
  Graphite, Forest, and Steel Blue. Font selection must allow any installed
  Windows screen font exposed by the platform font picker. Font size is adjusted
  through that picker, accepts 4 through 288 pt, and is not exposed as a separate
  Settings menu.
- Apply dark-family themes across the editor surface, line numbers, find/search
  panels, command palette, status area, and window chrome where Windows exposes
  that boundary.
- Keep native menus readable in dark mode. A dark menu background must not be
  applied unless menu text, disabled text, checks, and submenu arrows can be
  rendered with matching contrast.
- Let users move the active tab left or right.
- Preserve each tab's editor viewport and selection when the user switches away
  and returns during the same editing session, including the caret/selection
  range and first visible line.
- Let users open a file-backed active tab in a new j3Text window from
  tab-oriented UI surfaces. The new window is a separate process launched with
  the document path; unsaved in-memory text is transferred only if the user
  saves before launching. From the tab context menu, the source tab closes after
  the new process starts, and the action is disabled when only one tab is open.
- Let users close the active tab, close all tabs, or close every tab except the
  active tab while preserving dirty-tab save/discard/cancel decisions.
- Expose close-tab and close-other-tabs actions from tab-oriented UI surfaces
  and from the command palette so users do not have to discover them only under
  File.
- Support common editor keyboard shortcuts for file, search, tab, edit-control,
  and view commands. Users can change, restore, or disable each shortcut from
  Settings.
- Detect external target changes at Save or Save As time by comparing the last
  file snapshot with the current target metadata and content fingerprint.
- Treat a deleted or moved saved target as a save conflict. Keep the in-memory
  text and require Reload, Save As, or Cancel before overwriting another
  process's change.
- Save file-backed documents with an atomic replace policy: write encoded bytes
  to a unique same-directory temporary file, flush it, then replace the target.
  If any step fails before replacement, keep the previous file untouched.
- Persist settings and recent files through the same
  temporary-file-then-replace policy. Settings use the executable-adjacent TOML
  file, while recent files use the per-user data root.
  These failures remain non-fatal to editing state.
- Separate save failure causes from user-facing messages for permission denied,
  file in use, read-only targets, missing paths, encoding conversion warnings,
  and unknown I/O failures.
- Detect read-only files at open time and keep the document in read-only mode
  until the user saves through another path or reopens a writable copy.
- Let users show spaces and tabs without mutating the source document.
- Support a result panel for all search matches and navigation between results.
- In the find input, pressing Enter invokes the same Find Next command as the
  Next button. Pressing Esc in either the find or replace input closes the find
  bar through the same command path as the close button. After Enter, focus
  remains in the find input so repeated Enter presses keep advancing matches
  while the document selection remains visible.
- Open dropped files as tabs through the same open policy as menu/file-dialog
  opens.
- Expose a command palette backed by application commands for common File, Edit,
  Search, View, Tabs, and Document operations.
- Keep large file search and save flows bounded enough that the UI can keep
  painting status/timer events between user actions where possible.
- Keep document status updates and search result collection linear in the active
  document size. Status UI must use cached document metrics instead of rescanning
  the full buffer on every timer tick.
- Treat direct Save/Save As commands as background save jobs. The UI keeps
  processing paint/timer messages while the job writes the same-document
  snapshot; dirty-prompt saves may still block because the caller needs the save
  decision before closing or replacing a tab.
- Avoid full-buffer synchronization from the Win32 edit control on every edit
  notification. Edit notifications mark the document dirty and defer the full
  text copy until a use case needs the current text, such as save, search, tab
  switch, or closing prompt.
- Bundle the application icon into the Windows executable and use the same icon
  for the main editor window.
- Make the Windows GUI process DPI-aware before creating windows. Prefer
  Per-Monitor V2 awareness when the host OS supports it, create editor fonts
  and fixed UI dimensions from the current window DPI, and refresh the window
  rectangle, font, theme, and layout on `WM_DPICHANGED` outside native
  size/move loops.
- During native Win32 window move or resize loops, defer DPI rectangle feedback
  and DPI-dependent UI rebuilds until `WM_EXITSIZEMOVE`
  so mixed-DPI monitor transitions stay smooth.
- Create the main editor window at an initial 600 x 500 logical pixel size,
  scaled to the startup DPI.
- Center modal pop-up windows over the main editor window. Future pop-ups must
  use the same platform dialog boundary so placement stays consistent.

## Domain Terms

- Document: editable plain text plus identity, optional file path, selected
  encoding, selected line ending, dirty state, read-only state, and last saved
  or loaded file snapshot.
- StartupFile: a file path supplied on the process command line. Startup files
  are platform input that become normal document-open or new-file-with-path use
  cases after the main window is created.
- Tab: an ordered view slot for one `Document`. A dirty tab is shown with a
  leading `*`. A file-backed tab exposes its current path from the tab hover
  tooltip, wrapping long paths across multiple lines.
- RecentFiles: most recently opened or saved file paths, ordered newest first and
  deduplicated.
- Encoding: the file I/O encoding used for loading and saving. Supported values
  are UTF-8, UTF-8 BOM, UTF-16 LE, UTF-16 BE, EUC-KR, CP949, Shift-JIS,
  GB18030, Big5, ISO-8859-1, Windows-1250, Windows-1251, Windows-1252,
  Windows-1253, Windows-1254, Windows-1255, Windows-1256, Windows-1257, and
  Windows-874.
- LineEnding: the persisted newline sequence for save operations. Supported
  values are CRLF, LF, and CR.
- Dirty State: whether the in-memory document differs from the last saved or
  loaded file state. The app/domain layer owns this comparison through a clean
  baseline of plain text, encoding, and line ending. Rich Edit's own modify flag
  is not used as the source of truth because it remains set after Undo returns
  the visible text to the loaded or saved baseline.
- FileSnapshot: conservative file identity data used for save-conflict
  detection. It stores byte length and a modified-time marker that may include
  a content fingerprint so same-size target changes can still be detected.
- SavePolicy: the domain rule for how a document may be persisted. The default
  policy is `AtomicReplace`: encode and normalize content first, write a
  unique same-directory temporary file, flush it, then replace the target path.
  A failed prepare/write/flush leaves the target path unchanged.
- ReadOnlyDocument: a document state where edits and ordinary Save are blocked.
  Reasons are distinct: large file policy, file-system read-only attribute,
  or explicit application policy.
- SearchResult: one match in a document with byte range, one-based line/column,
  UTF-16 selection range, and a short preview string. Multi-result search
  returns an ordered capped set so large files do not produce unbounded UI work
  and result navigation does not have to rescan from the start of the document.
- PendingEditSync: a platform/app boundary state where the visible Win32 edit
  control has newer text than the app `Document`. The document is already dirty,
  but the full text copy is delayed until a use case needs exact content.
- EditorViewState: platform-owned per-document UTF-16 caret/selection range and
  first visible editor line. It is transient UI state for open tabs, not
  persisted across restarts.
- EditorSurfaceState: app-owned current editor-surface projection updated by the
  platform after meaningful editor events. It records the active document id,
  selection range, one-based line/column, and Undo/Redo availability without
  exposing Rich Edit notification names above the platform adapter.
- CurrentEditorStatus: app-owned UX state projection that combines `Document`,
  `EditorSettings`, background save state, and `EditorSurfaceState`. Menus and
  the status bar read this projection for dirty/clean, save availability,
  current path, read-only/effective-read-only, Word Wrap, Undo/Redo, and
  line/column state.
- DpiMetrics: platform-owned current window DPI state used to derive editor
  fonts and fixed UI dimensions. Layout uses this cached value instead of
  recomputing DPI on every resize message.
- SizeMoveDpiState: platform-owned state for the native Win32 size/move loop.
  It records deferred DPI refresh and status timer work so those tasks can run
  once after `WM_EXITSIZEMOVE`.
- PendingSave: a platform-owned background save job for a captured document
  content snapshot. The active document is made read-only while its save job is
  running so a completed save cannot clear newer edits.
- Command: a named use case the UI can execute. Commands are grouped by File,
  Edit, Search, View, Tabs, and Document, and the command palette invokes the
  same application actions as menus.
- KeyboardShortcut: a user-configurable key combination that invokes an editor
  command. Shortcuts are stored in editor settings, are unique per command, and
  must not bypass the command's normal app/platform handler.
- ApplicationIcon: the Windows executable icon resource used for Explorer,
  taskbar, Alt-Tab, and the main window title bar.
- EditorSettings: user preferences for font, tab size, word wrap, and theme.
  Font settings store the selected Windows screen font face name and point size.
  Shortcut settings store command key bindings such as the close-tab shortcut.
- ResolvedTheme: the effective concrete theme after applying the user's System
  preference. System follows the platform app/theme preference when it can be
  read, resolving to Classic Dark for dark system mode and Light otherwise.
- LargeFilePolicy: files at or above 100 MB require confirmation and are opened
  read-only when accepted.
- InMemoryDocumentBuffer: the current editor model stores the loaded document
  text in memory and uses the Win32 edit control as the view. Large-file support
  therefore optimizes common paths and blocks write-heavy operations, but true
  multi-hundred-MB editing requires a future paged or virtualized document model.

## Core Rules

- A new document starts as UTF-8, CRLF, clean, writable, and untitled.
- A new document created for a missing startup path starts as UTF-8, CRLF, clean,
  writable, and file-backed by that path with no file snapshot. Its first Save
  writes to the supplied path without opening Save As, but fails with an external
  file change if another process creates that path first.
- Editing document text marks the document dirty unless the document is read-only.
- Saving a document writes text using the document encoding and line ending, then
  marks it clean and updates the file snapshot.
- Saving uses `SavePolicy::AtomicReplace` for file-backed paths through a unique
  same-directory temporary file. The target file is not removed before the
  replacement step.
- A read-only document cannot be edited or saved through ordinary Save. Save As
  may create a writable document when the selected target is writable.
- Read-only documents also reject encoding and line-ending mutation. Those
  changes affect the next persisted bytes and are treated as writable-document
  commands.
- Visible whitespace mode is a read-only presentation mode. It blocks editing
  and ordinary Save while keeping Save As available for the already-synced real
  document text.
- A file with the read-only attribute opens as a `ReadOnlyDocument` even when it
  is smaller than the large file threshold.
- Permission denied, sharing violation/file-in-use, read-only target, not found,
  and generic I/O failures are classified before being shown to users.
- While an application-owned modal dialog is open, timer-driven status work is
  paused so save and persistence boundaries do not re-enter the same
  user decision.
- Startup file arguments are opened at startup.
- Missing file paths supplied as startup arguments ask whether to create the
  file.
- Opening a path that is already owned by an open file-backed tab does not
  create another tab. The app selects the existing tab for that path. If the
  existing tab was loaded from disk, the path is recorded as a recent file.
- Visible whitespace changes presentation only. The source text, dirty state,
  search ranges, and save output remain based on the real document text.
- Switching tabs saves the outgoing tab's `EditorViewState` before changing the
  active document and restores the incoming tab's state after loading its text
  into the Win32 edit control. Rich Edit이 문서 텍스트를 프로그램 내부에서
  다시 싣는 동안 보내는 선택/스크롤 알림은 저장된 `EditorViewState`를
  덮어쓰지 않는다.
- Hovering a file-backed tab shows the document's full current path in the tab
  tooltip. Long paths wrap across multiple lines, and untitled tabs do not show
  a path tooltip.
- Opening the active tab in a new window requires a document path and at least
  two open tabs. Dirty tabs ask whether to save first, then launch a separate
  j3Text process with that path and close the source tab.
- Multi-result search works on the document text, caps the number of results,
  and stores enough range data to move the caret without rescanning. Result
  collection must advance line/column state as matches are found instead of
  recalculating each match location from the start of the document. It also
  records UTF-16 ranges during the same pass because Win32 edit selections are
  UTF-16 based.
- Command palette execution must route through the same app/platform handlers as
  menu commands so command behavior stays consistent.
- Changing encoding or line ending marks a writable document dirty because the
  next save output changes.
- Opening auto-detects BOM encodings first, then BOM-less UTF-16 by NUL-pattern
  heuristics, then UTF-8, then Korean, Japanese, and Chinese code pages by
  round-trip checks where possible, falling back to ISO-8859-1.
- Reloading a clean externally changed file uses the same automatic open
  detection policy. Only explicit Reopen commands force an encoding.
- A user-selected encoding bypasses auto detection for the target reopen flow.
  For example, forcing UTF-8 on UTF-8 BOM bytes keeps the leading U+FEFF in
  document text; only the UTF-8 BOM encoding strips the BOM marker.
- Encoding selection accepts only supported encoding names. Manual input is
  matched case-insensitively with common hyphen, underscore, and space variants,
  and the dialog tells the user whether the typed value is valid before it can be
  accepted. Invalid input cannot trap the user in the dialog; Cancel, Esc, and
  the window close button all dismiss without applying an encoding.
- Save As prompts for the output encoding after the target path is chosen. The
  selected encoding becomes the document encoding after a successful save.
- Convert prompts for the output encoding before changing the active document's
  selected encoding. It does not write immediately; the next save writes through
  the selected encoding.
- Encoding conversion must fail before writing if the selected target encoding
  cannot represent the text. The app boundary turns that into a user warning.
- Files at or above 100 MB are not read until the user confirms. Confirmed large
  files are opened read-only to avoid slow edit/save paths in this step.
- Loading keeps the current in-memory document model but avoids duplicate UTF-8
  buffers where possible. Large files remain read-only because editing and
  ordinary Save still require full-buffer synchronization with the Win32 edit
  control.
- Saving Unicode encodings writes normalized text to the same-directory temporary
  file in bounded chunks, then atomically replaces the target. Save must not
  build both a full normalized string and a full encoded byte buffer for UTF-8 or
  UTF-16 output.
- Saving legacy code page encodings also normalizes and encodes in bounded text
  chunks. Encoding failure leaves the target file untouched and removes the
  temporary file.
- A direct Save/Save As command captures the current document content as a cheap
  immutable snapshot and writes it on a worker thread. While the save is pending,
  the saved document is read-only and commands that could replace, close, or
  mutate the tab are blocked until the worker reports success or failure.
- When Save fails because the target changed outside j3Text, the user gets
  `Reload`, `Save As`, and `Cancel` choices instead of a passive error. `Reload`
  selects that tab and replaces its text from disk, `Save As` selects that tab
  and runs the ordinary Save As flow for the in-memory text, and `Cancel` leaves
  the document unchanged.
- Dirty-prompt saves may use the blocking save path because close/open/exit
  decisions depend on the completed result. This is a deliberate boundary, not
  the common toolbar/menu Save path.
- Closing dirty tabs, replacing a dirty current tab, and exiting with dirty
  documents must ask whether to save, discard, or cancel.
- Keyboard shortcuts execute the same command paths as menus and the command
  palette. A changed shortcut replaces the command default; a disabled shortcut
  is not intercepted from the editor text control. Settings must prevent
  ambiguous duplicate shortcut assignments.
- Edit commands are plain text surface commands. The platform
  `EditorTextSurface` maps Undo, Redo, Cut, Copy, Paste, and Select All to the
  current Win32/Rich Edit implementation internally. These commands must not
  expose rich formatting or RTF insertion paths.
- The editor text surface context menu exposes the same Undo, Redo, Cut, Copy,
  Paste, and Select All commands as the Edit menu. Its enabled state follows
  `CurrentEditorStatus` so read-only, visible-whitespace, and save-in-progress
  states cannot run text-mutating commands from the menu.
- Undo/Redo availability originates from the current Rich Edit text surface's
  undo stack, but the platform converts it into app `EditorSurfaceState`. The
  app layer then combines that with read-only, visible-whitespace, and
  save-in-progress state before menus or status display can enable commands.
- Text-mutating editor commands, specifically Undo, Redo, Cut, and Paste, sync
  the current Rich Edit plain text back to the app immediately after the command
  returns. This keeps command-driven clean/dirty transitions, including Undo
  back to the saved baseline, visible without copying the full buffer on every
  ordinary `EN_CHANGE` notification.
- Find-input Enter handling must route through the same platform command path as
  the Next button so empty-query focus, result-panel navigation, and no-match
  messaging stay consistent. The find input must keep focus after Enter, with
  the document match selection still visible, so repeated Enter presses advance
  to the next match. Find/replace-input Esc handling must route through the same
  platform command path as the find-bar close button so the search result panel
  and layout are cleared consistently.
- Closing all tabs prompts for each dirty document before the tab set is reset to
  a new untitled document. Closing other tabs prompts only for dirty non-active
  documents and keeps the active document selected.
- Settings and recent-file persistence errors are non-fatal to
  editing. They may be reported, but must not block document operations.
- Windows `WM_DPICHANGED` outside a native size/move loop may apply the suggested window
  rectangle and rebuild DPI-dependent UI. Inside the loop, it only marks a
  pending DPI refresh; the actual font, theme, line-number, and layout refresh
  uses the current window DPI after `WM_EXITSIZEMOVE`.
- Font settings are sanitized before use: an empty font face falls back to the
  default editor font, and font size is clamped to 4 through 288 pt.
- Dark-family theme presentation follows the j3TreeText palettes. Classic Dark
  uses panel `#1f2124`, editor/input `#181a1d`, text `#e6e8eb`, and muted
  `#5c6169`. Sepia Teal uses panel `#181918`, editor/input `#1f3438`, text
  `#ece8db`, and muted `#b29a7c`. Graphite uses panel `#18191a`,
  editor/input `#32373f`, text `#efece5`, and muted `#7e7769`. Forest uses
  panel `#161917`, editor/input `#273b3f`, text `#ecefe5`, and muted
  `#689675`. Steel Blue uses panel `#18191b`, editor/input `#364050`, text
  `#eff0f2`, and muted `#688bab`.
- Native Windows menus use system-rendered colors unless the platform can apply
  a complete readable dark menu treatment. Partial dark menu backgrounds are not
  valid because the default menu text color may stay dark.
- j3Text does not poll open files for external changes in the background. External
  target changes are detected when Save or Save As checks the target expectation
  before replacing the file.
- If the backing path for a saved document is deleted before Save, the save
  expectation treats that as an external change unless the document was created
  specifically for a missing path and still expects the target to be absent.
- Settings and recent-file persistence errors must not
  mutate open documents, dirty state, or the editor's in-memory recent-file list.
- Win32 handles and control messages belong to `platform` and must not appear in
  `domain`.
- Rich Edit classes, DLL names, `EM_*`/`WM_*` constants, `CHARFORMAT`/
  `PARAFORMAT`-style structures, and RTF stream/formatting flags belong to the
  Win32 platform adapter. They must not appear in `app`, `domain`, or infra
  persistence contracts.
- The Windows application icon is resource id `1`; platform startup loads it
  from the executable and attaches it to the registered main window class.
- Platform modal dialogs, including message boxes and file dialogs, are owned by
  each backend and transient to the main editor window when the toolkit supports
  that boundary.
- The About dialog title is `About j3Text`, and its top version label is
  `j3Text <package version>`, with the version value sourced from Cargo package
  metadata. It exposes `about.txt` in a fixed-size read-only scrollable body,
  keeps the project URL as a bottom-left button, and closes through a bottom-right
  `OK` button. Its target size is 450 x 400, and long notices do not resize the
  About dialog.

## Use Case Boundaries

- entry: `main.rs` starts the OS-selected platform backend and reports fatal
  startup errors.
- app: owns document/tab use cases, recent-file ordering, and
  current state coordination, search-result state,
  and command dispatch decisions without doing file or platform UI I/O.
- domain: owns pure document, encoding, line-ending, settings, save policy,
  read-only reason, file snapshot, large-file policy,
  dirty-state, search, whitespace rendering, and command rules.
- infra: loads and saves documents, writes temporary files, performs byte/text
  encoding conversion, reads file metadata, and persists user settings,
  and recent-file state. Infra returns concrete errors and does not
  decide UI prompts.
- platform: owns Win32 or GTK4 windows, menus, dialogs, controls, drag/drop,
  startup file arguments, message loop/timers, process-local editor view state,
  OS API calls needed by infra, save-conflict prompts, and conversion between UI
  events and app use cases.
