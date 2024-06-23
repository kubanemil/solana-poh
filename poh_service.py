import time

from kafka import KafkaConsumer

from poh_recorder import PohRecorder, Record

TARGET_SLOT_ADJUSTMENT_NS: int = 50_000_000
TARGET_TICK_DURATION_NS: int = 400_000_000
TICKS_PER_SLOT = 64


class PohService:
    def tick_producer(
        self,
        poh_recorder: PohRecorder,
        hashes_per_batch: int,
        record_receiver,
    ):
        adjustment_per_tick = TARGET_SLOT_ADJUSTMENT_NS / TICKS_PER_SLOT
        target_ns_per_tick = TARGET_TICK_DURATION_NS - adjustment_per_tick

        next_record = None
        start = time.time()
        while True:
            should_tick, next_record = self.record_or_hash(
                next_record,
                poh_recorder,
                record_receiver,
                hashes_per_batch,
                target_ns_per_tick,
            )
            # print(should_tick, next_record)
            if should_tick:
                poh_recorder.tick()

                if time.time() - start > 30:
                    break

    def record_or_hash(
        self,
        next_record: Record | None,
        poh_recorder: PohRecorder,
        record_receiver: KafkaConsumer,
        hashes_per_batch: int,
        target_ns_per_tick: int,
    ) -> tuple[bool, Record | None]:
        if next_record:
            while next_record:
                poh_recorder.record(next_record.mixin, next_record.transactions)
                next_record = get_record(record_receiver)
            return False, next_record

        poh = poh_recorder.poh
        while True:
            should_tick = poh.hash(hashes_per_batch)
            ideal_time = poh.target_poh_time(target_ns_per_tick)
            if should_tick:
                return True, None

            record = get_record(record_receiver)
            if record:
                return False, record

            if ideal_time <= time.time():
                continue

            while ideal_time > time.time():
                record = get_record(record_receiver)
                if record:
                    return False, record
            break

        return False, next_record


def get_record(record_receiver: KafkaConsumer) -> Record | None:
    new_record = record_receiver.poll(timeout_ms=0.00001)
    if not new_record:
        return None
    for msg in new_record.values():
        record_value = msg[0].value.decode("utf-8")
        new_record = Record.from_json(record_value)
        return new_record


if __name__ == "__main__":
    poh_service = PohService()
    poh_recorder = PohRecorder(0, b"init", 10_000)
    record_receiver = KafkaConsumer("record", bootstrap_servers="localhost:9092")
    poh_service.tick_producer(poh_recorder, 200, record_receiver)
