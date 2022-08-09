app "helloWorld"
    packages { pf: "platform/main.roc" }
    imports [Decode, Decode.{Decoder, Decoding, DecoderFormatting}, Json]
    provides [main] to pf

theDecoder : Decoder {first: a, second: b} fmt | a has Decoding, b has Decoding, fmt has DecoderFormatting
theDecoder =
    Decode.custom \bytes, fmt ->
        Decode.decodeWith
            bytes
            (Decode.record
                {f0: Err NoField, f1: Err NoField}
                (\state, field ->
                    when field is
                        "first" ->
                            Keep (Decode.custom \bytes1, fmt1 ->
                                when Decode.decodeWith bytes1 Decode.decoder fmt1 is
                                    rec ->
                                        {
                                            result: (when rec.result is
                                                        Ok val -> Ok {state & f0: Ok val}
                                                        Err e -> Err e),
                                            rest: rec.rest
                                        })
                        "second" ->
                            Keep (Decode.custom \bytes1, fmt1 ->
                                when Decode.decodeWith bytes1 Decode.decoder fmt1 is
                                    rec ->
                                        {
                                            result: (when rec.result is
                                                        Ok val -> Ok {state & f1: Ok val}
                                                        Err e -> Err e),
                                            rest: rec.rest
                                        })
                        _ -> Skip)
                (\stateRecord ->
                    when stateRecord.f0 is
                        Ok first ->
                            when stateRecord.f1 is
                                Ok second -> Ok {first, second}
                                Err NoField -> Err TooShort
                        Err NoField -> Err TooShort))
            fmt

main =
    when Str.toUtf8 "{\"first\":\"ab\",\"second\":[\"cd\",\"ef\"]}" |> Decode.decodeWith theDecoder Json.fromUtf8 is
        {result, rest: _} ->
            when result is
                Ok { first, second } -> Str.concat first (Str.joinWith second ",")
                Err _ -> "<bad>"
