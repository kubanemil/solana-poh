Python was chosen for this implementation due to its readability and simplicity. 

However, for the sake of simplicity, the codebase omits several advanced features such as PoH chain verification, GPU/CPU efficient functions, comprehensive banking, reporting, thread message sending, slotting logic, leader-schedule related methods, and other advanced banking modules.

For this implementation, entries are just added to a bank list. In original code
records send via sender thread, however, in this implementation kafka is used for simplicity.