#!/usr/bin/env python3
"""Fix direct streaming blocks in vLLM's combined Inkling parser.

Adapted from the community spark-vllm mod (inkling-fix-direct-streaming-tool-calls
v1) with anchors corrected to match the eugr image's actual vllm/parser/inkling.py
(the forum-extracted copy had blank lines stripped, so its anchors matched 0 sites).
Semantics unchanged from the original mod:
  1. REASONING gains direct transitions to TEXT_START / TOOL_TEXT / TOOL_ERROR so
     a direct visible block closes reasoning instead of leaking markup.
  2. InklingParser._preprocess_feed promotes MESSAGE_HEADER -> REASONING when a
     direct block marker arrives under skip_tool_parsing, so the reasoning pass
     stops suppressing the tool pass for tool calls emitted with no thinking block.
"""
from __future__ import annotations

import ast
import sys
from pathlib import Path

MARKER = "# spark-vllm mod: inkling-fix-direct-streaming-tool-calls v1 (anchors adapted)"

REASONING_TRANSITIONS = '''        (ParserState.REASONING, "THINK_START"): Transition(
            ParserState.REASONING,
            (),
        ),
'''

PATCHED_REASONING_TRANSITIONS = '''        (ParserState.REASONING, "THINK_START"): Transition(
            ParserState.REASONING,
            (),
        ),
        # A model may emit a visible-text block without a thinking block.
        # Keep that direct path consistent with the direct-tool transition.
        (ParserState.REASONING, "TEXT_START"): Transition(
            ParserState.CONTENT,
            (EventType.REASONING_END,),
        ),
        (ParserState.REASONING, "TOOL_TEXT"): Transition(
            ParserState.CONTENT,
            (EventType.REASONING_END,),
        ),
        (ParserState.REASONING, "TOOL_ERROR"): Transition(
            ParserState.CONTENT,
            (EventType.REASONING_END,),
        ),
'''

CLASS_METHOD_ANCHOR = '''        kwargs.setdefault("parser_engine_config", inkling_config())
        super().__init__(tokenizer, tools, **kwargs)

    def adjust_initial_state_from_prompt(self, prompt_token_ids: Sequence[int]) -> None:
'''

PATCHED_CLASS_METHOD_ANCHOR = f'''        kwargs.setdefault("parser_engine_config", inkling_config())
        super().__init__(tokenizer, tools, **kwargs)

    {MARKER}
    def _preprocess_feed(
        self,
        delta_text: str,
        delta_token_ids: Sequence[int],
    ) -> tuple[str, Sequence[int]]:
        # DelegatingParser considers reasoning open after a model header, but
        # MESSAGE_HEADER must first suppress Inkling's optional function-name
        # metadata. Promote only when the direct block marker itself arrives.
        if (
            self.skip_tool_parsing
            and self._engine.state == ParserState.MESSAGE_HEADER
        ):
            direct_markers = (
                CONTENT_TEXT,
                CONTENT_INVOKE_TOOL_JSON,
                CONTENT_INVOKE_TOOL_TEXT,
                CONTENT_TOOL_ERROR,
            )
            direct_ids = {{
                token_id
                for marker in direct_markers
                if (token_id := self.vocab.get(marker)) is not None
            }}
            if any(token_id in direct_ids for token_id in delta_token_ids) or any(
                marker in delta_text for marker in direct_markers
            ):
                self._engine.state = ParserState.REASONING
        return super()._preprocess_feed(delta_text, delta_token_ids)

    def adjust_initial_state_from_prompt(self, prompt_token_ids: Sequence[int]) -> None:
'''


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise ValueError(f"expected exactly one {label}; found {count}")
    return text.replace(old, new, 1)


def validate(text: str) -> None:
    tree = ast.parse(text)
    parser_classes = [
        node
        for node in tree.body
        if isinstance(node, ast.ClassDef) and node.name == "InklingParser"
    ]
    if len(parser_classes) != 1:
        raise ValueError(
            f"expected exactly one InklingParser class; found {len(parser_classes)}"
        )
    compile(text, "<patched inkling.py>", "exec")


def patched_text(text: str) -> str:
    validate(text)
    if MARKER in text:
        if (
            'ParserState.REASONING, "TEXT_START"' not in text
            or "def _preprocess_feed(" not in text
        ):
            raise ValueError("mod marker exists but the parser fix is incomplete")
        return text
    text = replace_once(
        text,
        REASONING_TRANSITIONS,
        PATCHED_REASONING_TRANSITIONS,
        "reasoning transition block",
    )
    text = replace_once(
        text,
        CLASS_METHOD_ANCHOR,
        PATCHED_CLASS_METHOD_ANCHOR,
        "InklingParser method anchor",
    )
    validate(text)
    return text


def main() -> int:
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} INKLING_PARSER", file=sys.stderr)
        return 2
    target = Path(sys.argv[1])
    if not target.is_file():
        print(f"[inkling parser fix ERROR] target not found: {target}", file=sys.stderr)
        return 1
    original = target.read_text()
    try:
        patched = patched_text(original)
    except (SyntaxError, ValueError) as exc:
        print(
            f"[inkling parser fix ERROR] refusing to patch {target}: {exc}",
            file=sys.stderr,
        )
        return 1
    if patched == original:
        print("[inkling parser fix] Patch already applied; skipping.")
        return 0
    temporary = target.with_suffix(target.suffix + ".streaming-tool-fix.tmp")
    temporary.write_text(patched)
    temporary.replace(target)
    print(f"[inkling parser fix] Patched {target}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
