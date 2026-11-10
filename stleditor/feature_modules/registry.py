"""Feature evaluators that live in their own file. Each module calls register() on import."""
EVALUATORS = {}
LABELS = {}

def register(key, evaluator, label):
    EVALUATORS[key] = evaluator
    LABELS[key] = label
