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
            print("mixin:", next_record.mixin.hex())
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


if __name__ == "__main__":
    from hashlib import sha256

    init_hash = sha256(b"initial").digest()
    hashes_per_tick = 1_000_000

    poh_service = PohService()
    poh_recorder = PohRecorder(0, init_hash, hashes_per_tick)
    record_receiver = KafkaConsumer("record", bootstrap_servers="localhost:9092")
    try:
        poh_service.tick_producer(poh_recorder, hashes_per_tick, record_receiver)
    except KeyboardInterrupt:
        record_receiver.close()
        print("Tick entry num:", len(poh_recorder.tick_cache))
        print("Record entry num:", len(poh_recorder.bank.bank))
        print(poh_recorder.tick_height, poh_recorder.ticks_from_record)
