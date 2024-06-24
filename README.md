Python was chosen for this implementation due to its readability and simplicity. 

However, for the sake of simplicity, the codebase omits several advanced features such as PoH chain verification, GPU/CPU efficient functions, comprehensive banking, reporting, thread message sending, slotting logic, leader-schedule related methods, and other advanced banking modules.

For this implementation, entries are just added to a bank list. In original code
records send via sender thread, however, in this implementation kafka is used for simplicity.

To run the code, install the docker and compose it:
```bash
$ docker compose up
```

Wait few seconds till PoH service will run and go to the logs of container `main_service-1`.
You will see how ticks are generated and when mixins are added (they are sent from `tx_generator_service-1`)

To see detailed info about records sent, go to logs of `tx_generator_service-1`.


Also, here is the [blog](docs/PohBlog.md).