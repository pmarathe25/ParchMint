# ParchMint stage delivery

Complete the following passes in order, delegating one sub-agent to each pass. 
Unless otherwise specified, use the sub-agent type recommended in the table in 
`plans/README.md`. Give each sub-agent the relevant stage document from the 
`plans/` directory.

### Step 1. Independent test pass

Launch an `independent-test` sub-agent to write tests *before* implementing the stage. 
The sub-agent should refer to stage plan and the linked product and design requirements 
documents in order to write its tests. 

Tests are not expected to pass at this stage since the implementation is not yet ready. 
Ensure you tell the sub-agent NOT to run or even compile the tests it added!

### Step 2. Implementation pass

Launch an `implementation` sub-agent to write the implementation and integration/unit 
tests for the stage. This sub-agent should refer to the stage document and any linked
architecture documents to write an implementation that fulfills the documented requirements.

### Step 3. Simplification and test reconciliation pass

Launch a `simplification` sub-agent to:
- Remove redundant or duplicate tests
- Simplify code to reduce complexity 
- Refactor code to reduce duplication - DRY principle 
- Simplify any verbose comments or docstrings using plain and simple language;
  you may wish to mention the `write-plain-technical-docs` skill.

### Step 4. Verification and repair pass

1. Create a temporary `reports/stageXXX/revisionYYY` directory to communicate verification results
    between the sub-agents for the stage. Do not write any files into this directory besides
    the ones explicitly specified below. The revision number will start at `0` and increment during
    each new verification and repair pass for a given stage.

2. Dispatch a `spark_analyst` sub-agent to run all newly added tests and write
    a single concise, timestamped report in the `reports/stageXXX/revisionYYY` directory. The
    report should include the failing test names and the relevant error messages/stack traces.

3. If the GUI has already been implemented, also dispatch an `analyst` sub-agent to 
    validate the application behavior and generate a separate, single report file
    in the `reports/stageXXX/revisionYYY` directory detailing the defects it found.

4. Communicate the report files back to the `implementation` sub-agent so it can repair the defects.

5. Repeat this process until all bugs have been fixed. If defects remain after more than
    3 rounds of this process, stop and ask me what to do.

### Step 5. Commit and proceed

Once all tests for the stage pass and the application behavior has been verified, 
ask the implementing sub-agent to commit the changes. 

Then, proceed with the next stage, launching a new set of sub-agents following exactly
the same process above.
