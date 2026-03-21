"""
Random X->Y training demo - learns to map random inputs to random targets.

Generates random X and Y with no relationship, then trains a simple
linear model via gradient descent. Forward pass uses xos.nn.Linear (Burn-backed).
Gradient calculation remains manual.
"""
import xos

# Config
N_SAMPLES = 100
LEARNING_RATE = 0.01
EPOCHS = 500
PRINT_EVERY = 100


def main():
    xos.print("Random X->Y Training Demo (Burn Linear forward)")
    xos.print("=" * 40)

    # Generate random X and Y (no relationship - purely random)
    X = xos.tensor(
        [[xos.random.uniform(-1, 1)] for _ in range(N_SAMPLES)],
        (N_SAMPLES, 1)
    )
    Y = xos.tensor(
        [[xos.random.uniform(-1, 1)] for _ in range(N_SAMPLES)],
        (N_SAMPLES, 1)
    )

    xos.print(f"Generated {N_SAMPLES} random (X, Y) pairs")
    xos.print(f"X range: [{min(X['_data']):.3f}, {max(X['_data']):.3f}]")
    xos.print(f"Y range: [{min(Y['_data']):.3f}, {max(Y['_data']):.3f}]")
    xos.print("")

    # Linear model: forward uses Burn
    model = xos.nn.Linear(1, 1)
    # Override init to match original (w=0.5, b=0.0)
    model._weight[0] = 0.5
    model._bias[0] = 0.0

    x_data = X["_data"]
    y_data = Y["_data"]

    xos.print("Training (Burn forward + manual gradients)...")
    xos.print("")

    for epoch in range(EPOCHS):
        # Forward pass via Burn
        y_pred_batch = model(X)
        y_pred_list = y_pred_batch["_data"]

        total_loss = 0.0
        grad_w = 0.0
        grad_b = 0.0

        for i in range(N_SAMPLES):
            x = x_data[i]
            y_true = y_data[i]
            y_pred = y_pred_list[i]
            loss = (y_pred - y_true) ** 2
            total_loss += loss

            # Gradients for MSE: d/dw = 2*x*(y_pred - y_true), d/db = 2*(y_pred - y_true)
            err = y_pred - y_true
            grad_w += 2 * x * err
            grad_b += 2 * err

        avg_loss = total_loss / N_SAMPLES
        grad_w /= N_SAMPLES
        grad_b /= N_SAMPLES

        # Gradient descent step
        model._weight[0] -= LEARNING_RATE * grad_w
        model._bias[0] -= LEARNING_RATE * grad_b

        if (epoch + 1) % PRINT_EVERY == 0 or epoch == 0:
            w, b = model._weight[0], model._bias[0]
            xos.print(f"  Epoch {epoch + 1:4d} | Loss: {avg_loss:.6f} | w={w:.4f}, b={b:.4f}")

    xos.print("")
    xos.print("Training complete!")
    w, b = model._weight[0], model._bias[0]
    xos.print(f"Final model: y = {w:.4f} * x + {b:.4f}")
    xos.print("")
    xos.print("(Since X and Y are unrelated random data, this model")
    xos.print(" has no predictive value - it just memorized noise.)")


if __name__ == "__main__":
    main()
