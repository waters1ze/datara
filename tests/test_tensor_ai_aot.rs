use forgen::driver::ForgenCompiler;

#[test]
fn test_tensor_type_and_fused_kernel() {
    let source = r#"
class NetworkWeights {
    layers: Int
}

class TensorModel {
    weights: NetworkWeights
}

behavior TensorModel {
    @fused_kernel
    fn conv2d_bias_relu(in_channels: Int, out_channels: Int) -> Int {
        return in_channels * out_channels
    }

    @fused_kernel
    fn matmul_relu(m: Int, k: Int, n: Int) -> Int {
        return m * n
    }

    fn forward(self, input_dim: Int) -> Int {
        let c = TensorModel.conv2d_bias_relu(64, 128)
        let m = TensorModel.matmul_relu(1, 128, 10)
        return c + m
    }
}

fn main() -> Int {
    let model = TensorModel {
        weights: NetworkWeights { layers: 50 }
    }
    return model.forward(64)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.check_source(source, "tensor_ai_test.dtr");
    assert!(
        res.success,
        "@fused_kernel and Tensor operations should check cleanly: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_tensor_aot_compilation_to_dmir() {
    let source = r#"
class AIInferenceEngine {
    batch_size: Int
}

behavior AIInferenceEngine {
    @fused_kernel
    fn predict(batch: Int) -> Int {
        return batch * 42
    }
}

fn main() -> Int {
    return AIInferenceEngine.predict(8)
}
"#;
    let compiler = ForgenCompiler::new("release");
    let dmir = compiler.compile_source_to_dmir(source, "ai_aot.dtr");
    assert!(
        dmir.is_ok(),
        "Failed to lower AI AOT to DMIR: {:?}",
        dmir.err()
    );
    let module = dmir.unwrap();
    assert!(
        module.functions.contains_key("predict")
            || module.functions.contains_key("AIInferenceEngine_predict")
            || module.functions.contains_key("main")
    );
}
