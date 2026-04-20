---
name: test-reasoning
description: Validate that reasoning parameters are correctly serialized and sent to provider APIs. Use when the user asks to test reasoning serialization, run reasoning tests, verify reasoning config fields, or check that ReasoningConfig maps correctly to provider-specific JSON.
---

# Test Reasoning Serialization

Validates that `ReasoningConfig` fields are correctly serialized into provider-specific JSON for OpenAI-compatible providers.

## Quick Start

Run all tests with the bundled script:

```bash
./scripts/test-reasoning.sh
```

The script builds forge in debug mode, runs each provider/model combination, captures the
outgoing HTTP request body via `FORGE_DEBUG_REQUESTS`, and asserts the correct JSON fields.

## Running a Single Test Manually

```bash
FORGE_DEBUG_REQUESTS="forge.request.json" \
FORGE_SESSION__PROVIDER_ID=<provider_id> \
FORGE_SESSION__MODEL_ID=<model_id> \
FORGE_REASONING__EFFORT=<effort> \
target/debug/forge -p "Hello!"
```

Then inspect `.forge/forge.request.json` for the expected fields.

## Test Coverage

| Provider         | Model              | Config fields                                     | Expected JSON field               |
| ---------------- | ------------------ | ------------------------------------------------- | --------------------------------- |
| `forge`          | `openai/o4-mini`   | `effort: none\|minimal\|low\|medium\|high\|xhigh` | `reasoning.effort`                |
| `forge`          | `openai/o4-mini`   | `max_tokens: 4000`                                | `reasoning.max_tokens`            |
| `forge`          | `openai/o4-mini`   | `effort: high` + `exclude: true`                  | `reasoning.effort` + `.exclude`   |
| `forge`          | `openai/o4-mini`   | `enabled: true`                                   | `reasoning.enabled`               |
| `openai`         | `openai/o4-mini`   | `max_tokens: 4000`                                | `reasoning.max_tokens`            |
| `openai`         | `openai/o4-mini`   | `effort: high`                                    | `reasoning.effort`                |
| `llama_cpp`      | `llama-cpp-model`  | `max_tokens: 4000`                                | `reasoning.max_tokens`            |
| `ollama`         | `ollama-model`     | `effort: high`                                    | `reasoning.effort`                |
| `vllm`           | `vllm-model`       | `max_tokens: 4000`                                | `reasoning.max_tokens`            |
| all providers    | one model each     | `effort: invalid`                                 | non-zero exit, no request written |

Tests for unconfigured providers are skipped automatically. Invalid-effort tests run regardless of credentials — the rejection happens at config parse time before any provider interaction.

## References

- [OpenAI Reasoning guide](https://developers.openai.com/api/docs/guides/reasoning)
- [OpenAI Chat Completions API reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)
