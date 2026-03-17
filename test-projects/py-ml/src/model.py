"""Model building, training, and inference."""

from __future__ import annotations

import logging
import pickle
from pathlib import Path
from typing import Any

import numpy as np
from sklearn.base import BaseEstimator
from sklearn.ensemble import GradientBoostingClassifier, RandomForestClassifier
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score, f1_score, precision_score, recall_score

logger = logging.getLogger("py-ml.model")


def build_model(
    model_type: str = "random_forest",
    params: dict[str, Any] | None = None,
) -> BaseEstimator:
    """Build a scikit-learn model instance.

    Args:
        model_type: Type of model ('random_forest', 'gradient_boosting', 'logistic').
        params: Hyperparameters to pass to the model constructor.

    Returns:
        An unfitted scikit-learn estimator.

    Raises:
        ValueError: If model_type is not recognized.
    """
    params = params or {}

    model_registry: dict[str, type[BaseEstimator]] = {
        "random_forest": RandomForestClassifier,
        "gradient_boosting": GradientBoostingClassifier,
        "logistic": LogisticRegression,
    }

    if model_type not in model_registry:
        available = ", ".join(model_registry.keys())
        raise ValueError(f"Unknown model type '{model_type}'. Available: {available}")

    model_class = model_registry[model_type]

    # Apply sensible defaults per model type
    defaults: dict[str, dict[str, Any]] = {
        "random_forest": {"n_estimators": 100, "max_depth": 10, "n_jobs": -1, "random_state": 42},
        "gradient_boosting": {"n_estimators": 100, "max_depth": 5, "learning_rate": 0.1, "random_state": 42},
        "logistic": {"max_iter": 1000, "random_state": 42, "solver": "lbfgs"},
    }

    merged_params = {**defaults.get(model_type, {}), **params}
    model = model_class(**merged_params)

    logger.info("Built %s model with params: %s", model_type, merged_params)
    return model


def compile_model(
    model: BaseEstimator,
    optimization: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Prepare model configuration and validate parameters before training.

    Args:
        model: The scikit-learn estimator to compile.
        optimization: Optional optimization settings (cross-validation, scoring, etc.).

    Returns:
        Dictionary with compiled model configuration.
    """
    config: dict[str, Any] = {
        "model": model,
        "model_type": type(model).__name__,
        "params": model.get_params(),
        "scoring": "f1_weighted",
        "cv_folds": 5,
    }

    if optimization:
        if "scoring" in optimization:
            config["scoring"] = optimization["scoring"]
        if "cv_folds" in optimization:
            config["cv_folds"] = max(2, min(20, optimization["cv_folds"]))
        if "early_stopping" in optimization:
            config["early_stopping"] = optimization["early_stopping"]

    # Validate param ranges
    params = model.get_params()
    warnings: list[str] = []

    if "n_estimators" in params and params["n_estimators"] > 1000:
        warnings.append("n_estimators > 1000 may cause slow training")
    if "max_depth" in params and params["max_depth"] is not None and params["max_depth"] > 30:
        warnings.append("max_depth > 30 risks overfitting")

    if warnings:
        for w in warnings:
            logger.warning("compile_model: %s", w)
        config["warnings"] = warnings

    logger.info("Model compiled: %s (scoring=%s, cv=%d)", config["model_type"], config["scoring"], config["cv_folds"])
    return config


def fit(
    model: BaseEstimator,
    X_train: np.ndarray,
    y_train: np.ndarray,
    X_val: np.ndarray | None = None,
    y_val: np.ndarray | None = None,
) -> dict[str, Any]:
    """Train the model and return training metrics.

    Args:
        model: The estimator to fit.
        X_train: Training features.
        y_train: Training labels.
        X_val: Optional validation features.
        y_val: Optional validation labels.

    Returns:
        Dictionary with training results and metrics.
    """
    if X_train.shape[0] != y_train.shape[0]:
        raise ValueError(
            f"X_train and y_train must have same number of samples "
            f"(got {X_train.shape[0]} vs {y_train.shape[0]})"
        )

    logger.info(
        "Training %s on %d samples (%d features)",
        type(model).__name__,
        X_train.shape[0],
        X_train.shape[1],
    )

    model.fit(X_train, y_train)

    # Training metrics
    train_pred = model.predict(X_train)
    results: dict[str, Any] = {
        "model": model,
        "train_samples": X_train.shape[0],
        "n_features": X_train.shape[1],
        "train_accuracy": float(accuracy_score(y_train, train_pred)),
        "train_f1": float(f1_score(y_train, train_pred, average="weighted", zero_division=0)),
    }

    # Validation metrics
    if X_val is not None and y_val is not None:
        val_pred = model.predict(X_val)
        results["val_accuracy"] = float(accuracy_score(y_val, val_pred))
        results["val_f1"] = float(f1_score(y_val, val_pred, average="weighted", zero_division=0))
        results["val_precision"] = float(precision_score(y_val, val_pred, average="weighted", zero_division=0))
        results["val_recall"] = float(recall_score(y_val, val_pred, average="weighted", zero_division=0))

        overfit_gap = results["train_accuracy"] - results["val_accuracy"]
        if overfit_gap > 0.1:
            logger.warning(
                "Possible overfitting: train_acc=%.3f, val_acc=%.3f (gap=%.3f)",
                results["train_accuracy"],
                results["val_accuracy"],
                overfit_gap,
            )

    logger.info("Training complete: train_acc=%.3f", results["train_accuracy"])
    return results


def predict(
    model: BaseEstimator,
    X: np.ndarray,
    return_proba: bool = False,
) -> dict[str, np.ndarray]:
    """Generate predictions from a trained model.

    Args:
        model: Trained estimator.
        X: Feature matrix for prediction.
        return_proba: If True, include class probabilities.

    Returns:
        Dictionary with 'predictions' array and optionally 'probabilities'.
    """
    if X.shape[0] == 0:
        logger.warning("Empty input array, returning empty predictions")
        return {"predictions": np.array([])}

    predictions = model.predict(X)
    result: dict[str, np.ndarray] = {"predictions": predictions}

    if return_proba and hasattr(model, "predict_proba"):
        probabilities = model.predict_proba(X)
        result["probabilities"] = probabilities
        result["confidence"] = np.max(probabilities, axis=1)

        low_confidence = np.mean(result["confidence"] < 0.5)
        if low_confidence > 0.2:
            logger.warning(
                "%.1f%% of predictions have confidence < 0.5",
                low_confidence * 100,
            )

    logger.info("Generated %d predictions", len(predictions))
    return result


def save_model(
    model: BaseEstimator,
    output_path: str,
    metadata: dict[str, Any] | None = None,
) -> Path:
    """Serialize and save a trained model to disk.

    Args:
        model: The trained model to save.
        output_path: File path for the saved model.
        metadata: Optional metadata to store alongside the model.

    Returns:
        Path to the saved model file.
    """
    path = Path(output_path)
    path.parent.mkdir(parents=True, exist_ok=True)

    payload = {
        "model": model,
        "model_type": type(model).__name__,
        "params": model.get_params(),
        "metadata": metadata or {},
    }

    # Add feature importance if available
    if hasattr(model, "feature_importances_"):
        importances = model.feature_importances_
        top_indices = np.argsort(importances)[-10:][::-1]
        payload["top_feature_indices"] = top_indices.tolist()
        payload["top_feature_importances"] = importances[top_indices].tolist()

    with open(path, "wb") as f:
        pickle.dump(payload, f, protocol=pickle.HIGHEST_PROTOCOL)

    size_mb = path.stat().st_size / (1024 * 1024)
    logger.info("Model saved to %s (%.2f MB)", path, size_mb)
    return path
