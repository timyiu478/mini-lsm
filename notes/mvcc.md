OCC in MVCC:

* read-only transaction has NO validation because read-only transactions are immune to concurrent changes
* read validation
    * why? prevent write skew case:
        * T1: read x and y
        * T2: read x and y
        * T1: write x
        * T2: write y
    * CANNOT prevent write skew case: range scan
