#!/bin/bash

QOICONV_REF=./qoi/qoiconv
QOICONV_TARGET=./rapid-qoi/target/release/qoiconv
IMAGES=./qoi_benchmark_suite/images/screenshot_game

rm -f /tmp/{ref,target}.qoi /tmp/{ref,target}.{raw,target}.{raw,png}

result=0
find ${IMAGES} | grep -E '.png$' | while read x; do

  # Encoding

  ${QOICONV_REF} "${x}" /tmp/ref.qoi
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : encode failed by reference encoder. skip...
    rm /tmp/ref.qoi
    continue
  fi
  ${QOICONV_TARGET} "${x}" /tmp/target.qoi
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : encode failed by target encoder. skip...
    rm /tmp/{ref,target}.qoi
    continue
  fi

  # Decoding

  ${QOICONV_REF} /tmp/ref.qoi /tmp/ref.ref.png
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : decode failed by reference decoder
    rm /tmp/{ref,target}.qoi /tmp/ref.ref.png
    result=1
    continue
  fi
  ${QOICONV_REF} /tmp/target.qoi /tmp/target.ref.png
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : decode failed by reference decoder
    rm /tmp/{ref,target}.qoi /tmp/ref.{ref,target}.png
    result=1
    continue
  fi


  ${QOICONV_TARGET} /tmp/ref.qoi /tmp/ref.target.png
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : decode failed by target decoder
    rm /tmp/{ref,target}.qoi /tmp/{ref,target}.{ref,target}.png
    result=1
    continue
  fi
  ${QOICONV_TARGET} /tmp/target.qoi /tmp/target.target.png
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : decode failed by target decoder
    rm /tmp/{ref,target}.qoi /tmp/{ref,target}.{ref,target}.png
    result=1
    continue
  fi

  # Extracting

  ${QOICONV_TARGET} /tmp/ref.ref.png /tmp/ref.ref.raw
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : extraction failed
    rm /tmp/{ref,target}.qoi /tmp/{ref,target}.{ref,target}.{png,raw}
    result=1
    continue
  fi
  ${QOICONV_TARGET} /tmp/target.ref.png /tmp/target.ref.raw
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : extraction failed
    rm /tmp/{ref,target}.qoi /tmp/{ref,target}.{ref,target}.{png,raw}
    result=1
    continue
  fi
  ${QOICONV_TARGET} /tmp/ref.target.png /tmp/ref.target.raw
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : extraction failed
    rm /tmp/{ref,target}.qoi /tmp/{ref,target}.{ref,target}.{png,raw}
    result=1
    continue
  fi
  ${QOICONV_TARGET} /tmp/target.target.png /tmp/target.target.raw
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : extraction failed
    rm /tmp/{ref,target}.qoi /tmp/{ref,target}.{ref,target}.{png,raw}
    result=1
    continue
  fi

  # Comparing

  diff /tmp/ref.ref.raw /tmp/target.ref.raw
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : encode by target encoder is not correct
    result=1
  fi

  diff /tmp/ref.ref.raw /tmp/ref.target.raw
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : decode by target decoder is not correct
    result=1
  fi

  diff /tmp/ref.ref.raw /tmp/target.target.raw
  ret=$?
  if [ ${ret} -ne 0 ]; then
    echo ${x} : roundtrip by target encoder-decoder is not correct
    result=1
  fi

  # Cleanup

  rm /tmp/{ref,target}.qoi /tmp/{ref,target}.{ref,target}.{png,raw}
done

exit ${result}
