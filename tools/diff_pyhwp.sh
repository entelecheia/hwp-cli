#!/usr/bin/env bash
# 차등 테스트: 우리 `hwp cat` vs pyhwp `hwp5txt`.
#
# 사용법: tools/diff_pyhwp.sh [파일들...]   (생략 시 fixtures/hwp5/*.hwp)
#
# pyhwp는 표/글상자 내용을 <표>/생략으로 처리하므로 완전 일치 게이트가
# 아니라 *우리 출력이 pyhwp 출력을 포함하는지* 보고하는 도구다.
# 불일치 = 우리 파서 버그 또는 pyhwp 한계 — 둘 다 가치 있는 발견.
set -uo pipefail
cd "$(dirname "$0")/.."

PYHWP=(uvx --python 3.9 --from pyhwp --with six hwp5txt)
FILES=("$@")
[ ${#FILES[@]} -eq 0 ] && FILES=(fixtures/hwp5/*.hwp)

normalize() {
    # 공백 정규화 + pyhwp 플레이스홀더 라인 제거
    sed 's/<표>//g; s/<그림>//g' | tr -s ' \t\n' ' ' | sed 's/^ *//; s/ *$//'
}

fail=0
for f in "${FILES[@]}"; do
    ours=$(cargo run -q -- cat "$f" 2>/dev/null | normalize)
    theirs=$("${PYHWP[@]}" "$f" 2>/dev/null | normalize)
    if [ -z "$theirs" ] && [ -z "$ours" ]; then
        echo "PASS(빈 문서)  $f"
    elif [ "$ours" = "$theirs" ]; then
        echo "PASS(일치)     $f"
    else
        # pyhwp 출력의 모든 단어가 우리 출력에 포함되는지 검사
        missing=0
        while IFS= read -r word; do
            [ -z "$word" ] && continue
            case "$ours" in *"$word"*) ;; *) missing=$((missing+1));; esac
        done <<< "$(echo "$theirs" | tr ' ' '\n' | sort -u)"
        if [ "$missing" -eq 0 ]; then
            echo "PASS(포함)     $f  (우리 출력이 pyhwp의 상위집합)"
        else
            echo "FAIL           $f  (pyhwp 단어 ${missing}개 누락)"
            fail=1
        fi
    fi
done
exit $fail
