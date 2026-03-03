"""LLM configuration for agent-lite.

Supports OpenAI-compatible API configuration via environment variables or CLI arguments.
"""

from dataclasses import dataclass
from urllib.parse import urlparse

import os

DEFAULT_TIMEOUT = 60


@dataclass(slots=True)
class LlmConfig:
    """OpenAI-compatible LLM configuration."""

    base_url: str = ""
    api_key: str = ""
    model: str = ""
    timeout: int = DEFAULT_TIMEOUT


def validate_config(config: LlmConfig) -> LlmConfig:
    """Validate LLM configuration.

    Args:
        config: Configuration to validate.

    Returns:
        Validated configuration.

    Raises:
        ValueError: If required fields are missing or invalid.
    """
    if not config.base_url:
        raise ValueError("base_url is required")

    try:
        parsed = urlparse(config.base_url)
        if not parsed.scheme or not parsed.netloc:
            raise ValueError(f"base_url must be a valid URL, got: {config.base_url}")
    except Exception as e:
        raise ValueError(f"base_url validation failed: {e}")

    if not config.api_key:
        raise ValueError("api_key is required")

    if config.timeout <= 0:
        raise ValueError("timeout must be positive")

    return config


def load_config(
    base_url: str | None = None,
    api_key: str | None = None,
    model: str | None = None,
    timeout: int | None = None,
) -> LlmConfig:
    """Load LLM configuration from environment variables and CLI arguments.

    CLI arguments take precedence over environment variables.

    Environment variables:
        ONE_API_BASE_URL: API endpoint URL
        ONE_API_TOKEN: API authentication token
        ONE_API_MODEL: Model name
        ONE_API_TIMEOUT: Request timeout in seconds

    Args:
        base_url: CLI-provided base_url (optional).
        api_key: CLI-provided api_key (optional).
        model: CLI-provided model (optional).
        timeout: CLI-provided timeout in seconds (optional).

    Returns:
        Validated LLM configuration.
    """
    env_base_url = os.environ.get("ONE_API_BASE_URL", "")
    env_api_key = os.environ.get("ONE_API_TOKEN", "")
    env_model = os.environ.get("ONE_API_MODEL", "")
    env_timeout = os.environ.get("ONE_API_TIMEOUT", "")

    final_base_url = base_url if base_url is not None else env_base_url
    final_api_key = api_key if api_key is not None else env_api_key
    final_model = model if model is not None else env_model

    if timeout is not None:
        final_timeout = timeout
    elif env_timeout:
        final_timeout = int(env_timeout)
    else:
        final_timeout = DEFAULT_TIMEOUT

    config = LlmConfig(
        base_url=final_base_url,
        api_key=final_api_key,
        model=final_model,
        timeout=final_timeout,
    )

    return validate_config(config)
