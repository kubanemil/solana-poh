from time import sleep

from kafka import KafkaConsumer

from poh_recorder import Record

consumer = KafkaConsumer("my_topic", bootstrap_servers="localhost:9092")
print("Consumer is running")

# Poll for a single message
while True:
    messages = consumer.poll(timeout_ms=1000)
    print(messages)
    if messages:
        for msg in messages.values():
            msg_value = msg[0].value.decode("utf-8")
            print(f"Consumed message: {msg_value}")
            record = Record.from_json(msg_value)
            print()
            print(record.mixin, record.transactions, record.slot)
    else:
        sleep(1)
