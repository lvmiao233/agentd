"""Tests for agent-lite LLM config injection.

TDD approach:
- RED: Tests for missing/invalid config first (should fail)
- GREEN: Implementation passes these tests
"""

import os
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from unittest.mock import patch

_CONFIG_PATH = (
    Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "config.py"
)
_CONFIG_SPEC = spec_from_file_location("agentd_agent_lite.config", _CONFIG_PATH)
assert _CONFIG_SPEC is not None and _CONFIG_SPEC.loader is not None
_CONFIG_MODULE = module_from_spec(_CONFIG_SPEC)
_CONFIG_SPEC.loader.exec_module(_CONFIG_MODULE)

load_config = _CONFIG_MODULE.load_config
validate_config = _CONFIG_MODULE.validate_config
LlmConfig = _CONFIG_MODULE.LlmConfig


def assert_raises_value_error(callable_obj, expected_substring: str) -> None:
    try:
        callable_obj()
    except ValueError as exc:
        assert expected_substring in str(exc)
        return
    raise AssertionError("expected ValueError was not raised")


class TestMissingConfig:
    """Tests for missing required configuration - should fail initially."""

    def test_missing_base_url_fails(self) -> None:
        """Missing base_url should raise validation error."""
        config = LlmConfig(api_key="test-key", model="gpt-4")
        assert_raises_value_error(lambda: validate_config(config), "base_url")

    def test_missing_api_key_fails(self) -> None:
        """Missing api_key should raise validation error."""
        config = LlmConfig(base_url="http://localhost:3000/v1", model="gpt-4")
        assert_raises_value_error(lambda: validate_config(config), "api_key")

    def test_empty_api_key_fails(self) -> None:
        """Empty api_key should raise validation error."""
        config = LlmConfig(
            base_url="http://localhost:3000/v1", api_key="", model="gpt-4"
        )
        assert_raises_value_error(lambda: validate_config(config), "api_key")


class TestInvalidConfig:
    """Tests for invalid configuration values - should fail initially."""

    def test_invalid_base_url_not_url_fails(self) -> None:
        """Invalid base_url (not a URL) should fail validation."""
        config = LlmConfig(base_url="not-a-url", api_key="test-key", model="gpt-4")
        assert_raises_value_error(lambda: validate_config(config), "base_url")

    def test_invalid_base_url_no_path_fails(self) -> None:
        """base_url without /v1 path should warn but not fail (OpenAI-compatible allows flexibility)."""
        config = LlmConfig(
            base_url="http://localhost:3000", api_key="test-key", model="gpt-4"
        )
        validated = validate_config(config)
        assert validated.base_url == "http://localhost:3000"


class TestValidConfig:
    """Tests for valid configuration - should pass."""

    def test_valid_config_passes(self) -> None:
        """Valid config should pass validation."""
        config = LlmConfig(
            base_url="http://localhost:3000/v1",
            api_key="test-key",
            model="gpt-4",
        )
        validated = validate_config(config)
        assert validated.base_url == "http://localhost:3000/v1"
        assert validated.api_key == "test-key"
        assert validated.model == "gpt-4"

    def test_default_timeout(self) -> None:
        """Default timeout should be 60 seconds."""
        config = LlmConfig(
            base_url="http://localhost:3000/v1",
            api_key="test-key",
            model="gpt-4",
        )
        validated = validate_config(config)
        assert validated.timeout == 60

    def test_default_model_fallback(self) -> None:
        """Model should fallback to default when not provided by CLI/env."""
        with patch.dict(os.environ, {}, clear=True):
            config = load_config(
                base_url="http://localhost:3000/v1", api_key="test-key"
            )
            assert config.model == "claude-4-sonnet"


class TestEnvVarInjection:
    """Tests for environment variable injection."""

    def test_env_base_url_injection(self) -> None:
        """Environment variable ONE_API_BASE_URL should inject base_url."""
        with patch.dict(
            os.environ,
            {"ONE_API_BASE_URL": "http://env:3000/v1", "ONE_API_TOKEN": "env-token"},
        ):
            config = load_config(model="gpt-4")
            assert config.base_url == "http://env:3000/v1"
            assert config.api_key == "env-token"

    def test_env_api_key_injection(self) -> None:
        """Environment variable ONE_API_TOKEN should inject api_key."""
        with patch.dict(os.environ, {"ONE_API_TOKEN": "secret-key"}):
            config = load_config(base_url="http://localhost:3000/v1", model="gpt-4")
            assert config.api_key == "secret-key"

    def test_env_model_injection(self) -> None:
        """Environment variable ONE_API_MODEL should inject model."""
        with patch.dict(os.environ, {"ONE_API_MODEL": "gpt-4o"}):
            config = load_config(
                base_url="http://localhost:3000/v1", api_key="test-key"
            )
            assert config.model == "gpt-4o"

    def test_env_timeout_injection(self) -> None:
        """Environment variable ONE_API_TIMEOUT should inject timeout."""
        with patch.dict(os.environ, {"ONE_API_TIMEOUT": "30"}):
            config = load_config(
                base_url="http://localhost:3000/v1", api_key="test-key", model="gpt-4"
            )
            assert config.timeout == 30


class TestCliOverride:
    """Tests for CLI argument override of env vars."""

    def test_cli_overrides_env(self) -> None:
        """CLI args should override environment variables."""
        with patch.dict(
            os.environ,
            {"ONE_API_BASE_URL": "http://env:3000/v1", "ONE_API_TOKEN": "env-token"},
        ):
            config = load_config(
                base_url="http://cli:3000/v1",
                api_key="cli-key",
                model="gpt-4",
            )
            assert config.base_url == "http://cli:3000/v1"
            assert config.api_key == "cli-key"
