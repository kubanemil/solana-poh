from random import randint
from time import sleep

from kafka import KafkaProducer

from entry import hash_transactions
from poh_recorder import Record, Transaction

producer = KafkaProducer(bootstrap_servers="localhost:9092")

idx = 0


def generate_random_tx():
    return Transaction(
        [f"sig{randint(0, 1000)}" for _ in range(randint(1, 7))],
        f"msg{randint(0, 100)}",
    )


while True:
    txs = [generate_random_tx() for i in range(randint(1, 10))]
    txs_hash = hash_transactions(txs)
    idx += 1

    record = Record(txs_hash, txs, 1)

    producer.send("record", record.to_json().encode())
    producer.flush()
    print(f"Sent: {record}")
    sleep(randint(1, 20) / 10)
