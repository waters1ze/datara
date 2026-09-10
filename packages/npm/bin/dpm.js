#!/usr/bin/env node
const { runBinary } = require('../lib/binary');
runBinary('dpm', process.argv.slice(2));
