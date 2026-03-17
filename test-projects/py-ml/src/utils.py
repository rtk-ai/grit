"""Utility functions for the ML pipeline."""

from __future__ import annotations

import json
import logging
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

import yaml


def setup_logging(
    level: str = "INFO",
    log_file: str | None = None,
    format_string: str | None = None,
) -> logging.Logger:
    """Configure logging for the pipeline.

    Args:
        level: Log level (DEBUG, INFO, WARNING, ERROR).
        log_file: Optional path to write logs to disk.
        format_string: Custom format string for log messages.

    Returns:
        Configured root logger instance.
    """
    fmt = format_string or "%(asctime)s [%(levelname)s] %(name)s: %(message)s"
    handlers: list[logging.Handler] = [logging.StreamHandler(sys.stdout)]

    if log_file:
        log_path = Path(log_file)
        log_path.parent.mkdir(parents=True, exist_ok=True)
        handlers.append(logging.FileHandler(str(log_path)))

    logging.basicConfig(
        level=getattr(logging, level.upper(), logging.INFO),
        format=fmt,
        handlers=handlers,
        force=True,
    )

    logger = logging.getLogger("py-ml")
    logger.info("Logging initialized at %s level", level)
    return logger


def load_config(config_path: str = "config.yaml") -> dict[str, Any]:
    """Load pipeline configuration from a YAML file.

    Args:
        config_path: Path to the YAML configuration file.

    Returns:
        Dictionary of configuration values.

    Raises:
        FileNotFoundError: If the config file doesn't exist.
        yaml.YAMLError: If the file contains invalid YAML.
    """
    path = Path(config_path)
    if not path.exists():
        raise FileNotFoundError(f"Configuration file not found: {config_path}")

    with open(path) as f:
        raw = yaml.safe_load(f)

    if not isinstance(raw, dict):
        raise ValueError(f"Config must be a YAML mapping, got {type(raw).__name__}")

    defaults = {
        "random_seed": 42,
        "test_size": 0.2,
        "batch_size": 32,
        "epochs": 10,
        "learning_rate": 0.001,
        "output_dir": "output",
    }

    merged = {**defaults, **raw}
    logging.getLogger("py-ml").info(
        "Loaded config from %s (%d keys)", config_path, len(merged)
    )
    return merged


def save_results(
    results: dict[str, Any],
    output_path: str,
    format: str = "json",
) -> Path:
    """Save pipeline results to disk.

    Args:
        results: Dictionary of results to save.
        output_path: Destination file path.
        format: Output format ('json' or 'yaml').

    Returns:
        Path to the saved file.
    """
    path = Path(output_path)
    path.parent.mkdir(parents=True, exist_ok=True)

    # Add metadata
    enriched = {
        "metadata": {
            "saved_at": datetime.now().isoformat(),
            "format": format,
            "keys": list(results.keys()),
        },
        **results,
    }

    with open(path, "w") as f:
        if format == "yaml":
            yaml.dump(enriched, f, default_flow_style=False, sort_keys=False)
        else:
            json.dump(enriched, f, indent=2, default=str)

    logging.getLogger("py-ml").info("Results saved to %s (%s)", path, format)
    return path


def plot_metrics(
    metrics: dict[str, list[float]],
    output_path: str = "output/metrics.png",
    title: str = "Training Metrics",
) -> Path:
    """Plot training metrics and save the figure.

    Args:
        metrics: Dictionary mapping metric names to lists of values per epoch.
        output_path: Path to save the plot image.
        title: Title for the plot.

    Returns:
        Path to the saved image.
    """
    import matplotlib.pyplot as plt

    fig, axes = plt.subplots(1, len(metrics), figsize=(6 * len(metrics), 4))
    if len(metrics) == 1:
        axes = [axes]

    for ax, (name, values) in zip(axes, metrics.items()):
        epochs = list(range(1, len(values) + 1))
        ax.plot(epochs, values, marker="o", linewidth=2)
        ax.set_title(name)
        ax.set_xlabel("Epoch")
        ax.set_ylabel(name)
        ax.grid(True, alpha=0.3)

    fig.suptitle(title, fontsize=14, fontweight="bold")
    fig.tight_layout()

    path = Path(output_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(str(path), dpi=150, bbox_inches="tight")
    plt.close(fig)

    logging.getLogger("py-ml").info("Metrics plot saved to %s", path)
    return path


def validate_schema(
    data: dict[str, Any],
    required_fields: list[str],
    field_types: dict[str, type] | None = None,
) -> tuple[bool, list[str]]:
    """Validate that data conforms to an expected schema.

    Args:
        data: The dictionary to validate.
        required_fields: List of required field names.
        field_types: Optional mapping of field names to expected types.

    Returns:
        Tuple of (is_valid, list_of_errors).
    """
    errors: list[str] = []

    for field in required_fields:
        if field not in data:
            errors.append(f"Missing required field: '{field}'")
        elif data[field] is None:
            errors.append(f"Field '{field}' must not be None")

    if field_types:
        for field, expected_type in field_types.items():
            if field in data and data[field] is not None:
                if not isinstance(data[field], expected_type):
                    actual = type(data[field]).__name__
                    errors.append(
                        f"Field '{field}' expected {expected_type.__name__}, got {actual}"
                    )

    is_valid = len(errors) == 0
    if not is_valid:
        logging.getLogger("py-ml").warning(
            "Schema validation failed: %d error(s)", len(errors)
        )

    return is_valid, errors
