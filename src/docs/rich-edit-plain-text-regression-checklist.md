# Rich Edit plain text 회귀 체크리스트

이 문서는 Rich Edit 기반 주 편집 표면이 j3Text의 plain text editor 계약을 계속
지키는지 확인하기 위한 회귀 기준이다. 자동 테스트로 잡는 항목은
`tests/plain_text_editor_regression.rs`, `tests/file_io_integration.rs`,
`tests/large_file_regression.rs`, 그리고 `src/platform/win32.rs`의 platform 단위
테스트를 우선한다. 아래 수동 체크는 실제 Windows UI와 클립보드, 메뉴, 파일
다이얼로그 동작이 필요한 항목을 고정한다.

## 수동 체크 전 준비

- `cargo test`가 통과한 빌드에서 실행한다.
- 테스트 파일은 임시 작업 폴더에 만든다.
- 각 체크 뒤 앱을 새로 시작하거나 새 탭을 만들어 이전 상태가 섞이지 않게 한다.

## 필수 수동 체크

- [ ] 새 문서 생성: 앱 시작 또는 File > New로 빈 문서를 만들면 본문은 비어 있고,
  UTF-8/CRLF 기본 상태이며 탭은 dirty 표시가 없다.
- [ ] 한글/영문/숫자/기호 입력: `한글 English 123 !@#$%^&*() [] {} <> "\\`
  를 입력하면 본문이 리터럴 plain text로 표시되고 탭 dirty 표시가 켜진다.
- [ ] 여러 줄 텍스트와 줄바꿈: 세 줄 이상을 입력하고 저장한 뒤 다시 열면 줄 순서와
  빈 줄이 그대로 보존된다.
- [ ] 파일 열기: UTF-8 텍스트 파일을 열면 내용, 인코딩 표시, line ending 표시가
  파일과 일치한다.
- [ ] 파일 저장: 새 문서를 Save As로 저장하면 파일 bytes가 입력한 plain text와
  선택된 line ending 정책만 반영한다.
- [ ] 저장 후 다시 열기: 저장한 파일을 닫고 다시 열었을 때 본문이 동일하고 탭 dirty
  표시가 꺼져 있다.
- [ ] 상태바 기본 상태: 새 문서와 열린 파일에서 상태바가 현재 줄/열, 문자 수,
  선택 문자 수, 인코딩, line ending, Word Wrap, 저장 가능 상태, UX 상태와 현재
  파일 경로를 표시한다. 새 문서는 `Saved`, `Can Save`, `Wrap Off`로 시작한다.
- [ ] 저장 가능 상태: writable 문서는 File > Save가 활성화되고 상태바가
  `Can Save`를 표시한다. visible whitespace 또는 read-only 문서에서는 ordinary
  Save가 비활성화되고 Save As만 가능한 경우 `Save As`로 표시된다. 저장 중에는
  Save/Save As가 모두 막히고 `No Save`와 `Saving` 상태가 보인다.
- [ ] 줄/열 표시: 커서 이동, 마우스 클릭, 선택 변경, 스크롤 뒤 상태바의 `Line`/`Col`
  값이 현재 캐럿 위치와 일치하고, 탭 전환 후 이전 탭의 위치가 복원된다.
- [ ] 붙여넣기: 브라우저나 Word 같은 서식 있는 원본에서 텍스트를 복사해 붙여넣으면
  글꼴, 굵게, 링크, RTF 서식이 들어오지 않고 Unicode plain text만 삽입된다.
- [ ] Undo/Redo: 입력, 붙여넣기, 선택 영역 치환 뒤 Undo/Redo를 실행하면 본문과 탭
  dirty 표시가 현재 저장 기준과 일치한다. Undo/Redo 메뉴 활성화도 상태바/본문과
  함께 갱신된다. Undo로 저장된 본문까지 돌아오면 dirty 표시가 꺼져야 한다.
- [ ] 변경 감지: 파일을 열고 외부 편집기로 같은 파일을 바꾼 뒤 j3Text로 돌아오면
  clean 문서는 외부 변경 경고 또는 reload 경로를 제공한다. dirty 문서는 사용자
  메모리 내용을 덮어쓰지 않는다.
- [ ] 읽기 전용 상태: 파일 시스템 read-only 속성이 켜진 파일을 열면 탭에
  read-only 상태가 표시되고 일반 입력, 붙여넣기, ordinary Save가 막힌다. Save As로
  쓰기 가능한 새 파일에 저장하면 새 기준은 writable clean 문서가 된다. Convert
  Encoding과 line ending 변경도 read-only 상태에서는 막힌다.
- [ ] Word Wrap: Word Wrap을 켜고 끄는 동안 본문, dirty 표시, 저장 bytes, 검색 결과
  range가 바뀌지 않는다. 메뉴 체크와 상태바의 `Wrap On`/`Wrap Off` 표시가 즉시
  갱신되고, 앱 재시작 후 설정 값은 유지된다.
- [ ] 큰 파일: 100 MB 이상 파일을 열면 읽기 전에 확인을 요구하고, 사용자가 계속하면
  read-only로 열린다. 128 MB 초과 파일은 확인 후에도 안전 상한 오류로 거부된다.
- [ ] RTF처럼 보이는 문자열: `{\\rtf1\\b text}`를 입력, 붙여넣기, 저장, 재열기해도
  서식으로 해석되지 않고 같은 문자열로 남는다.

## 자동화로 고정된 기준

- 새 문서 기본 상태, Save As, 저장 후 재열기, 한글/영문/숫자/기호, 여러 줄 텍스트,
  CRLF/LF 저장 정책은 `tests/plain_text_editor_regression.rs`가 검증한다.
- 최근 파일/Save As 재열기는 `tests/file_io_integration.rs`가 검증한다.
- 100 MB 경계, 확인된 큰 파일 read-only, 안전 read 상한은
  `tests/large_file_regression.rs`가 검증한다.
- Rich Edit plain text mode, RTF 리터럴 보존, Unicode 왕복, 선택 치환, Undo/Redo,
  Word Wrap 텍스트 불변성, read-only 표면 불변성은 `src/platform/win32.rs` 단위
  테스트가 검증한다.
