from hashlib import sha256
from poh_service import PohService
from poh_recorder import PohRecorder
from kafka import KafkaConsumer
from time import sleep
import os
import logging


sleep(int(os.environ["WAIT_TIME"]))
logging.basicConfig(level=logging.INFO)
KAFKA_SERVER = os.getenv("KAFKA_SERVER", "kafka:9093")
record_receiver = KafkaConsumer("record", bootstrap_servers=KAFKA_SERVER)

init_hash = sha256(b"initial").digest()
hashes_per_tick = 1_000_000

poh_service = PohService()
poh_recorder = PohRecorder(0, init_hash, hashes_per_tick)


poh_service.tick_producer(poh_recorder, hashes_per_tick, record_receiver)
