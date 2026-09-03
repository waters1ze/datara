#!/usr/bin/env node
const { runBinary } = require('../lib/binary');
runBinary('datara', process.argv.slice(2));
