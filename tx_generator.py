from random import randint
from time import sleep
import os
import logging

from kafka import KafkaProducer

from entry import hash_transactions
from poh_recorder import Record, Transaction


def start(producer):
    while True:
        txs = [generate_random_tx() for i in range(randint(1, 10))]
        txs_hash = hash_transactions(txs)

        record = Record(txs_hash, txs, 1)

        producer.send("record", record.to_json().encode())
        producer.flush()
        logging.info(f"Sent: {record}")
        sleep(randint(1, 20) / 10)


def generate_random_tx():
    return Transaction(
        [f"sig{randint(0, 1000)}" for _ in range(randint(1, 7))],
        f"msg{randint(0, 100)}",
    )


if __name__ == "__main__":
    sleep(int(os.environ["WAIT_TIME"]))
    logging.basicConfig(level=logging.INFO)
    producer = KafkaProducer(bootstrap_servers=os.getenv("KAFKA_SERVER", "kafka:9093"))
    start(producer)