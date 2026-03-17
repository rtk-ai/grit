"""Feature engineering and preprocessing functions."""

from __future__ import annotations

import logging
from typing import Any

import numpy as np
import pandas as pd

logger = logging.getLogger("py-ml.features")


def extract_features(
    df: pd.DataFrame,
    numeric_cols: list[str],
    categorical_cols: list[str] | None = None,
    date_cols: list[str] | None = None,
) -> pd.DataFrame:
    """Extract and engineer features from raw data.

    Args:
        df: Input dataframe with raw data.
        numeric_cols: Columns to treat as numeric features.
        categorical_cols: Columns to encode as categorical.
        date_cols: Columns to extract date-based features from.

    Returns:
        DataFrame with engineered features.
    """
    result = df.copy()

    # Numeric interaction features
    for i, col_a in enumerate(numeric_cols):
        for col_b in numeric_cols[i + 1 :]:
            if col_a in result.columns and col_b in result.columns:
                result[f"{col_a}_x_{col_b}"] = result[col_a] * result[col_b]
                result[f"{col_a}_div_{col_b}"] = result[col_a] / result[col_b].replace(0, np.nan)

    # Date features
    if date_cols:
        for col in date_cols:
            if col in result.columns:
                dt = pd.to_datetime(result[col], errors="coerce")
                result[f"{col}_year"] = dt.dt.year
                result[f"{col}_month"] = dt.dt.month
                result[f"{col}_dayofweek"] = dt.dt.dayofweek
                result[f"{col}_is_weekend"] = dt.dt.dayofweek.isin([5, 6]).astype(int)

    # Categorical encoding placeholder
    if categorical_cols:
        result = encode_categorical(result, categorical_cols)

    logger.info(
        "Extracted features: %d columns -> %d columns",
        len(df.columns),
        len(result.columns),
    )
    return result


def normalize(
    df: pd.DataFrame,
    columns: list[str],
    method: str = "standard",
) -> tuple[pd.DataFrame, dict[str, dict[str, float]]]:
    """Normalize numeric columns using the specified method.

    Args:
        df: Input dataframe.
        columns: Columns to normalize.
        method: Normalization method ('standard', 'minmax', 'robust').

    Returns:
        Tuple of (normalized dataframe, stats dict for inverse transform).
    """
    result = df.copy()
    stats: dict[str, dict[str, float]] = {}

    for col in columns:
        if col not in result.columns:
            logger.warning("Column '%s' not found, skipping normalization", col)
            continue

        values = result[col].astype(float)

        if method == "standard":
            mean = values.mean()
            std = values.std()
            if std == 0:
                logger.warning("Column '%s' has zero std, skipping", col)
                stats[col] = {"mean": mean, "std": 1.0}
                continue
            result[col] = (values - mean) / std
            stats[col] = {"mean": mean, "std": std}

        elif method == "minmax":
            vmin, vmax = values.min(), values.max()
            range_val = vmax - vmin
            if range_val == 0:
                stats[col] = {"min": vmin, "max": vmax}
                continue
            result[col] = (values - vmin) / range_val
            stats[col] = {"min": vmin, "max": vmax}

        elif method == "robust":
            median = values.median()
            q1 = values.quantile(0.25)
            q3 = values.quantile(0.75)
            iqr = q3 - q1
            if iqr == 0:
                stats[col] = {"median": median, "iqr": 1.0}
                continue
            result[col] = (values - median) / iqr
            stats[col] = {"median": median, "iqr": iqr}

    logger.info("Normalized %d columns using '%s' method", len(columns), method)
    return result, stats


def encode_categorical(
    df: pd.DataFrame,
    columns: list[str],
    max_categories: int = 20,
) -> pd.DataFrame:
    """One-hot encode categorical columns.

    Args:
        df: Input dataframe.
        columns: Categorical columns to encode.
        max_categories: Maximum unique values before switching to frequency encoding.

    Returns:
        DataFrame with encoded categorical columns.
    """
    result = df.copy()

    for col in columns:
        if col not in result.columns:
            continue

        n_unique = result[col].nunique()

        if n_unique > max_categories:
            # Frequency encoding for high-cardinality columns
            freq_map = result[col].value_counts(normalize=True).to_dict()
            result[f"{col}_freq"] = result[col].map(freq_map).fillna(0.0)
            result.drop(columns=[col], inplace=True)
            logger.info("Frequency-encoded '%s' (%d categories)", col, n_unique)
        else:
            # One-hot encoding
            dummies = pd.get_dummies(result[col], prefix=col, dtype=int)
            result = pd.concat([result.drop(columns=[col]), dummies], axis=1)
            logger.info("One-hot encoded '%s' (%d categories)", col, n_unique)

    return result


def handle_missing(
    df: pd.DataFrame,
    strategy: str = "median",
    fill_values: dict[str, Any] | None = None,
    drop_threshold: float = 0.5,
) -> pd.DataFrame:
    """Handle missing values in the dataframe.

    Args:
        df: Input dataframe.
        strategy: Imputation strategy ('mean', 'median', 'mode', 'drop', 'fill').
        fill_values: Custom fill values per column (used when strategy='fill').
        drop_threshold: Drop columns with missing ratio above this threshold.

    Returns:
        DataFrame with missing values handled.
    """
    result = df.copy()
    initial_shape = result.shape

    # Drop columns with too many missing values
    missing_ratio = result.isnull().mean()
    cols_to_drop = missing_ratio[missing_ratio > drop_threshold].index.tolist()
    if cols_to_drop:
        result.drop(columns=cols_to_drop, inplace=True)
        logger.info("Dropped %d columns with >%.0f%% missing values", len(cols_to_drop), drop_threshold * 100)

    # Impute remaining missing values
    for col in result.columns:
        if result[col].isnull().sum() == 0:
            continue

        if strategy == "fill" and fill_values and col in fill_values:
            result[col].fillna(fill_values[col], inplace=True)
        elif strategy == "drop":
            result.dropna(subset=[col], inplace=True)
        elif result[col].dtype in (np.float64, np.int64, float, int):
            if strategy == "mean":
                result[col].fillna(result[col].mean(), inplace=True)
            elif strategy == "median":
                result[col].fillna(result[col].median(), inplace=True)
            elif strategy == "mode":
                result[col].fillna(result[col].mode().iloc[0] if not result[col].mode().empty else 0, inplace=True)
        else:
            # Non-numeric: fill with mode
            mode_val = result[col].mode()
            result[col].fillna(mode_val.iloc[0] if not mode_val.empty else "unknown", inplace=True)

    logger.info(
        "Missing value handling: %s -> %s (strategy=%s)",
        initial_shape,
        result.shape,
        strategy,
    )
    return result


def create_embeddings(
    texts: list[str],
    max_features: int = 5000,
    method: str = "tfidf",
) -> np.ndarray:
    """Create text embeddings from a list of strings.

    Args:
        texts: List of text documents.
        max_features: Maximum vocabulary size.
        method: Embedding method ('tfidf', 'count', 'hash').

    Returns:
        NumPy array of shape (n_documents, n_features).
    """
    if not texts:
        logger.warning("Empty text list provided, returning empty array")
        return np.array([]).reshape(0, 0)

    # Clean texts
    cleaned = []
    for text in texts:
        t = str(text).lower().strip()
        t = "".join(c if c.isalnum() or c.isspace() else " " for c in t)
        t = " ".join(t.split())  # collapse whitespace
        cleaned.append(t)

    # Build vocabulary
    word_counts: dict[str, int] = {}
    for text in cleaned:
        for word in text.split():
            word_counts[word] = word_counts.get(word, 0) + 1

    # Select top features
    sorted_words = sorted(word_counts.items(), key=lambda x: -x[1])
    vocab = {word: idx for idx, (word, _) in enumerate(sorted_words[:max_features])}
    n_features = len(vocab)

    if method == "count":
        embeddings = np.zeros((len(cleaned), n_features), dtype=np.float32)
        for i, text in enumerate(cleaned):
            for word in text.split():
                if word in vocab:
                    embeddings[i, vocab[word]] += 1.0

    elif method == "tfidf":
        # Term frequency
        tf = np.zeros((len(cleaned), n_features), dtype=np.float32)
        for i, text in enumerate(cleaned):
            words = text.split()
            for word in words:
                if word in vocab:
                    tf[i, vocab[word]] += 1.0
            if words:
                tf[i] /= len(words)

        # Inverse document frequency
        doc_freq = np.sum(tf > 0, axis=0).astype(np.float32)
        idf = np.log((len(cleaned) + 1) / (doc_freq + 1)) + 1
        embeddings = tf * idf

    else:  # hash
        embeddings = np.zeros((len(cleaned), max_features), dtype=np.float32)
        for i, text in enumerate(cleaned):
            for word in text.split():
                idx = hash(word) % max_features
                embeddings[i, idx] += 1.0

    logger.info(
        "Created %s embeddings: shape=%s, vocab_size=%d",
        method,
        embeddings.shape,
        n_features if method != "hash" else max_features,
    )
    return embeddings
