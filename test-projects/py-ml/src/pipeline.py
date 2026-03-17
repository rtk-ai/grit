"""Main ML pipeline orchestration."""

from __future__ import annotations

import logging
import time
from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd
from sklearn.model_selection import train_test_split

from .features import extract_features, handle_missing, normalize
from .model import build_model, compile_model, fit, predict, save_model
from .utils import load_config, plot_metrics, save_results, setup_logging, validate_schema

logger = logging.getLogger("py-ml.pipeline")


def run_pipeline(config_path: str = "config.yaml") -> dict[str, Any]:
    """Execute the full ML pipeline from config to evaluation.

    Args:
        config_path: Path to the pipeline configuration file.

    Returns:
        Dictionary containing all pipeline results and metrics.
    """
    setup_logging(level="INFO")
    start_time = time.time()

    logger.info("Starting pipeline run")
    config = load_config(config_path)

    # Validate config schema
    valid, errors = validate_schema(
        config,
        required_fields=["data_path", "target_column", "model_type"],
        field_types={"test_size": float, "epochs": int},
    )
    if not valid:
        raise ValueError(f"Invalid pipeline config: {errors}")

    # Load and preprocess data
    raw_data = load_data(config["data_path"])
    processed = preprocess(raw_data, config)

    # Split features and target
    target_col = config["target_column"]
    X = processed.drop(columns=[target_col]).values
    y = processed[target_col].values

    X_train, X_test, y_train, y_test = train_test_split(
        X, y,
        test_size=config.get("test_size", 0.2),
        random_state=config.get("random_seed", 42),
        stratify=y if len(np.unique(y)) > 1 else None,
    )

    # Build and train model
    model = train_model(X_train, y_train, X_test, y_test, config)

    # Evaluate
    eval_results = evaluate(model, X_test, y_test)

    # Save artifacts
    output_dir = config.get("output_dir", "output")
    save_model(model, f"{output_dir}/model.pkl", metadata={"config": config})
    save_results(eval_results, f"{output_dir}/results.json")

    if "metrics_history" in eval_results:
        plot_metrics(eval_results["metrics_history"], f"{output_dir}/metrics.png")

    elapsed = time.time() - start_time
    logger.info("Pipeline completed in %.1f seconds", elapsed)

    return {
        "config": config,
        "evaluation": eval_results,
        "elapsed_seconds": elapsed,
        "data_shape": raw_data.shape,
        "train_size": len(y_train),
        "test_size": len(y_test),
    }


def load_data(data_path: str) -> pd.DataFrame:
    """Load data from various file formats.

    Args:
        data_path: Path to the data file (CSV, Parquet, or JSON).

    Returns:
        Loaded DataFrame.

    Raises:
        FileNotFoundError: If the data file doesn't exist.
        ValueError: If the file format is not supported.
    """
    path = Path(data_path)
    if not path.exists():
        raise FileNotFoundError(f"Data file not found: {data_path}")

    suffix = path.suffix.lower()
    loaders = {
        ".csv": lambda p: pd.read_csv(p),
        ".parquet": lambda p: pd.read_parquet(p),
        ".json": lambda p: pd.read_json(p),
        ".tsv": lambda p: pd.read_csv(p, sep="\t"),
    }

    if suffix not in loaders:
        supported = ", ".join(loaders.keys())
        raise ValueError(f"Unsupported file format '{suffix}'. Supported: {supported}")

    df = loaders[suffix](str(path))

    logger.info(
        "Loaded data from %s: %d rows x %d columns (%.1f MB)",
        path.name,
        df.shape[0],
        df.shape[1],
        df.memory_usage(deep=True).sum() / (1024 * 1024),
    )

    # Quick data quality check
    null_pct = df.isnull().mean().mean() * 100
    if null_pct > 20:
        logger.warning("Data has %.1f%% missing values overall", null_pct)

    duplicate_rows = df.duplicated().sum()
    if duplicate_rows > 0:
        logger.warning("Found %d duplicate rows (%.1f%%)", duplicate_rows, 100 * duplicate_rows / len(df))

    return df


def preprocess(
    df: pd.DataFrame,
    config: dict[str, Any],
) -> pd.DataFrame:
    """Run preprocessing steps on raw data.

    Args:
        df: Raw input dataframe.
        config: Pipeline configuration.

    Returns:
        Preprocessed dataframe ready for model training.
    """
    logger.info("Preprocessing %d rows x %d columns", df.shape[0], df.shape[1])

    # Handle missing values
    missing_strategy = config.get("missing_strategy", "median")
    result = handle_missing(df, strategy=missing_strategy)

    # Identify column types
    numeric_cols = result.select_dtypes(include=[np.number]).columns.tolist()
    categorical_cols = result.select_dtypes(include=["object", "category"]).columns.tolist()
    target_col = config.get("target_column", "")

    # Remove target from feature processing lists
    numeric_cols = [c for c in numeric_cols if c != target_col]
    categorical_cols = [c for c in categorical_cols if c != target_col]

    # Extract features
    date_cols = config.get("date_columns", [])
    result = extract_features(
        result,
        numeric_cols=numeric_cols,
        categorical_cols=categorical_cols,
        date_cols=date_cols,
    )

    # Normalize numeric features
    norm_method = config.get("normalization", "standard")
    final_numeric = [c for c in result.select_dtypes(include=[np.number]).columns if c != target_col]
    if final_numeric:
        result, _ = normalize(result, final_numeric, method=norm_method)

    logger.info("Preprocessing complete: %d rows x %d columns", result.shape[0], result.shape[1])
    return result


def train_model(
    X_train: np.ndarray,
    y_train: np.ndarray,
    X_val: np.ndarray,
    y_val: np.ndarray,
    config: dict[str, Any],
) -> Any:
    """Build, compile, and train a model.

    Args:
        X_train: Training features.
        y_train: Training labels.
        X_val: Validation features.
        y_val: Validation labels.
        config: Pipeline configuration.

    Returns:
        Trained model instance.
    """
    model_type = config.get("model_type", "random_forest")
    model_params = config.get("model_params", {})

    model = build_model(model_type, model_params)
    compiled = compile_model(model, config.get("optimization"))

    logger.info(
        "Training %s on %d samples, validating on %d",
        model_type,
        X_train.shape[0],
        X_val.shape[0],
    )

    results = fit(model, X_train, y_train, X_val, y_val)

    logger.info(
        "Training metrics: train_acc=%.3f, val_acc=%.3f",
        results.get("train_accuracy", 0),
        results.get("val_accuracy", 0),
    )

    return results["model"]


def evaluate(
    model: Any,
    X_test: np.ndarray,
    y_test: np.ndarray,
) -> dict[str, Any]:
    """Evaluate a trained model on the test set.

    Args:
        model: Trained model instance.
        X_test: Test features.
        y_test: True test labels.

    Returns:
        Dictionary with evaluation metrics.
    """
    from sklearn.metrics import classification_report, confusion_matrix

    predictions = predict(model, X_test, return_proba=True)
    y_pred = predictions["predictions"]

    report = classification_report(y_test, y_pred, output_dict=True, zero_division=0)
    cm = confusion_matrix(y_test, y_pred)

    results: dict[str, Any] = {
        "accuracy": report["accuracy"],
        "weighted_f1": report["weighted avg"]["f1-score"],
        "weighted_precision": report["weighted avg"]["precision"],
        "weighted_recall": report["weighted avg"]["recall"],
        "confusion_matrix": cm.tolist(),
        "n_test_samples": len(y_test),
        "n_classes": len(np.unique(y_test)),
        "per_class": {
            str(k): v
            for k, v in report.items()
            if k not in ("accuracy", "macro avg", "weighted avg")
        },
    }

    if "confidence" in predictions:
        results["mean_confidence"] = float(np.mean(predictions["confidence"]))
        results["low_confidence_pct"] = float(np.mean(predictions["confidence"] < 0.5) * 100)

    logger.info(
        "Evaluation: accuracy=%.3f, f1=%.3f (%d samples, %d classes)",
        results["accuracy"],
        results["weighted_f1"],
        results["n_test_samples"],
        results["n_classes"],
    )

    return results
