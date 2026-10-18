class VerifyError(ValueError):
    """An entry does not follow from the one before it."""

    def __init__(self, index: int, reason: str) -> None:
        super().__init__(f"entry {index}: {reason}")
        self.index = index
        self.reason = reason
