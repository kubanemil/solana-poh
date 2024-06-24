import logging

from kafka import KafkaConsumer

from poh_recorder import PohRecorder, Record

RECEIVER_TIMEOUT_MS = 10


class PohService:
    def tick_producer(
        self,
        poh_recorder: PohRecorder,
        hashes_per_tick: int,
        record_receiver: KafkaConsumer,
    ):
        while True:
            self.record_or_tick(
                poh_recorder,
                record_receiver,
                hashes_per_tick,
            )

    def record_or_tick(
        self,
        poh_recorder: PohRecorder,
        record_receiver: KafkaConsumer,
        hashes_per_tick: int,
    ):
        next_record = get_record(record_receiver)
        if next_record:
            poh_recorder.record(next_record.mixin, next_record.transactions)
            logging.info(f"mixin: {next_record.mixin.hex()}")
        else:
            # if no new records, will check if should tick or will try to get ne wrecord
            poh = poh_recorder.poh
            should_tick = poh.hash(hashes_per_tick)
            if should_tick:
                poh_recorder.tick()


def get_record(record_receiver: KafkaConsumer) -> Record | None:
    new_record = record_receiver.poll(timeout_ms=RECEIVER_TIMEOUT_MS)
    if not new_record:
        return None
    for msg in new_record.values():
        record_value = msg[0].value.decode("utf-8")
        new_record = Record.from_json(record_value)
        return new_record
