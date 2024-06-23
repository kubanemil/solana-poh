from kafka import KafkaProducer

from poh_recorder import Record, Transaction

producer = KafkaProducer(bootstrap_servers="localhost:9092")

record = Record(b"1234", [Transaction(["sig1", "sig2"], "msg1")], 1)
producer.send("record", record.to_json().encode())
producer.flush()
print(f"Sent: {record}")
