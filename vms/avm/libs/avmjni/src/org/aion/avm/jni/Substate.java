package org.aion.avm.jni;

import org.aion.vm.api.interfaces.KernelInterface;
import org.aion.types.Address;

import java.util.List;
import java.util.Set;
import java.util.ArrayList;
import java.util.function.Consumer;
import java.math.BigInteger;
import java.util.HashMap;
import java.util.HashSet;

public class Substate implements KernelInterface {
    final private KernelInterface parent;
    private final List<Consumer<KernelInterface>> writeLog;
    /// cached nonces
    private final HashMap<Address, BigInteger> nonces;
    /// cached balances
    private final HashMap<Address, BigInteger> balances;
    /// cached object graph
    private final HashMap<Address, byte[]> objectGraphs;
    /// storage keys and values
    private final HashMap<Address, HashSet<byte[]>> keys;
    private final HashMap<byte[], byte[]> values;
    
    /// block info (act as env info)
    private EnvInfo info;
    private boolean isLocalCall;

    private static final String ADDR_OWNER =
            "0000000000000000000000000000000000000000000000000000000000000000";
    private static final String ADDR_TOTAL_CURRENCY =
            "0000000000000000000000000000000000000000000000000000000000000100";

    private static final String ADDR_TOKEN_BRIDGE =
            "0000000000000000000000000000000000000000000000000000000000000200";
    private static final String ADDR_TOKEN_BRIDGE_INITIAL_OWNER =
            "a008d7b29e8d1f4bfab428adce89dc219c4714b2c6bf3fd1131b688f9ad804aa";

    private static final String ADDR_ED_VERIFY =
            "0000000000000000000000000000000000000000000000000000000000000010";
    private static final String ADDR_BLAKE2B_HASH =
            "0000000000000000000000000000000000000000000000000000000000000011";
    private static final String ADDR_TX_HASH =
            "0000000000000000000000000000000000000000000000000000000000000012";

    private class EnvInfo {
        private Address coinbase;
        private long blockTimestamp;
        private long blockDifficulty;
        private long blockGasLimit;
        private long blockNumber;
    }

    public Substate(KernelInterface parent, boolean isLocal) {
        this.parent = parent;
        this.writeLog = new ArrayList<>();
        this.nonces = new HashMap<>();
        this.balances = new HashMap<>();
        this.objectGraphs = new HashMap<>();
        this.keys = new HashMap<>();
        this.values = new HashMap<>();
        this.info = new EnvInfo();
        this.isLocalCall = isLocal;
    }

    // block info is regarded as EnvInfo for transactions
    public void updateEnvInfo(Message msg) {
        NativeDecoder decoder = new NativeDecoder(msg.blockDifficulty);
        this.info.blockDifficulty = decoder.decodeLong();
        this.info.blockTimestamp = msg.blockTimestamp;
        this.info.blockGasLimit = msg.blockEnergyLimit;
        this.info.blockNumber = msg.blockNumber;
        this.info.coinbase = new Address(msg.blockCoinbase);
    }

    private boolean isPrecompiledContract(Address address) {
        switch (address.toString()) {
            case ADDR_TOKEN_BRIDGE:
            case ADDR_ED_VERIFY:
            case ADDR_BLAKE2B_HASH:
            case ADDR_TX_HASH:
                return true;
            case ADDR_TOTAL_CURRENCY:
            default:
                return false;
        }
    }

    @Override
    public void createAccount(Address address) {
        if (Constants.DEBUG) {
            System.out.printf("JNI: create account: %s", address);
        }
        Consumer<KernelInterface> write = (kernel) -> {
            kernel.createAccount(address);
        };
        writeLog.add(write);
    }

    @Override
    public boolean hasAccountState(Address address) {
        if (Constants.DEBUG) {
            System.out.printf("JNI: check account state: %s", address);
        }
        return this.parent.hasAccountState(address);
    }

    @Override
    public void putCode(Address address, byte[] code) {
        if (Constants.DEBUG) {
            System.out.printf("JNI: save code: %s", address);
        }
        Consumer<KernelInterface> write = (kernel) -> {
            kernel.putCode(address, code);
        };
        writeLog.add(write);
    }

    @Override
    public byte[] getCode(Address address) {
        if (Constants.DEBUG) {
            System.out.printf("JNI: get code of %s", address);
        }
        return this.parent.getCode(address);
    }

    @Override
    public void putStorage(Address address, byte[] key, byte[] value) {
        if (Constants.DEBUG) {
            System.out.printf("JNI: put storage");
        }
        Consumer<KernelInterface> write = (kernel) -> {
            kernel.putStorage(address, key, value);
        };
        writeLog.add(write);

        HashSet<byte[]> keySet = this.keys.get(address);
        if (keySet == null) {
            this.keys.put(address, new HashSet<>());
            this.keys.get(address).add(key);
            this.values.put(key, value);
        } else {
            // key set is not null but the key is not found
            keySet.add(key);
            this.values.put(key, value);
        }

        
    }

    @Override
    public byte[] getStorage(Address address, byte[] key) {
        if (Constants.DEBUG) {
            System.out.printf("JNI: get storage");
        }

        byte[] value;
        HashSet<byte[]> localKeys = this.keys.get(address);
        if (null == localKeys) {
            value = this.parent.getStorage(address, key);
            this.keys.put(address, new HashSet<>());
            this.keys.get(address).add(key);
            this.values.put(key, value);
        } else {
            // has local keys
            if (localKeys.contains(key)) {
                value = this.values.get(key);
            } else {
                value = this.parent.getStorage(address, key);
                // key/value always update together
                localKeys.add(key);
                this.values.put(key, value);
            }
        }

        return value;
    }

    @Override
    public void deleteAccount(Address address) {
        Consumer<KernelInterface> write = (kernel) -> {
            kernel.deleteAccount(address);
        };
        writeLog.add(write);
    }

    @Override
    public boolean accountNonceEquals(Address address, BigInteger nonce) {
        if (Constants.DEBUG) {
            System.out.print("current Nonce = ");
            System.out.println(getNonce(address));
        }
        
        return this.isLocalCall || getNonce(address).compareTo(nonce) == 0;
    }

    @Override
    public BigInteger getBalance(Address address) {
        if (Constants.DEBUG) {
            System.out.printf("JNI: getBalance of ");
            System.out.println(address);
        }
        BigInteger balance = this.balances.get(address);
        if (null == balance) {
            balance = this.parent.getBalance(address);
            this.balances.put(address, balance);
        }
        return balance;
    }

    @Override
    public void adjustBalance(Address address, BigInteger delta) {
        if (Constants.DEBUG) {
            System.out.printf("try adjust balance: %d\n", delta.longValue());
        }
        Consumer<KernelInterface> write = (kernel) -> {
            kernel.adjustBalance(address, delta);
        };
        writeLog.add(write);

        this.balances.put(address, getBalance(address).add(delta));
    }

    @Override
    public BigInteger getNonce(Address address) {
        if (Constants.DEBUG) {
            System.out.print("JNI: try getNonce of: ");
            System.out.println(address);
        }
        
        BigInteger nonce = this.nonces.get(address);
        if (nonce == null) {
            nonce = this.parent.getNonce(address);
            this.nonces.put(address, nonce);
        }
        return nonce;
    }

    @Override
    public void incrementNonce(Address address) {
        Consumer<KernelInterface> write = (kernel) -> {
            kernel.incrementNonce(address);
        };
        writeLog.add(write);
        BigInteger nonce = this.nonces.get(address);
        if (nonce == null) {
            nonce = this.parent.getNonce(address);
        }
        this.nonces.put(address, nonce.add(BigInteger.ONE));
    }

    @Override
    public boolean accountBalanceIsAtLeast(Address address, BigInteger amount) {
        return this.isLocalCall || getBalance(address).compareTo(amount) >= 0;
    }
    
    @Override
    public boolean isValidEnergyLimitForNonCreate(long energyLimit) {
        return this.parent.isValidEnergyLimitForNonCreate(energyLimit);
    }

    @Override
    public boolean isValidEnergyLimitForCreate(long energyLimit) {
        return this.parent.isValidEnergyLimitForCreate(energyLimit);
    }

    @Override
    public boolean destinationAddressIsSafeForThisVM(Address address) {
        if (isPrecompiledContract(address)) {
            return false;
        }
        return this.parent.destinationAddressIsSafeForThisVM(address);
    }

    @Override
    public byte[] getBlockHashByNumber(long blockNumber) {
        return this.parent.getBlockHashByNumber(blockNumber);
    }

    @Override
    public void payMiningFee(Address address, BigInteger fee) {
        // This method may have special logic in the kernel. Here it is just adjustBalance.
        adjustBalance(address, fee);
    }

    @Override
    public void refundAccount(Address address, BigInteger amount) {
        // This method may have special logic in the kernel. Here it is just adjustBalance.
        adjustBalance(address, amount);
    }

    @Override
    public void deductEnergyCost(Address address, BigInteger cost) {
        // This method may have special logic in the kernel. Here it is just adjustBalance.
        adjustBalance(address, cost);
    }

    @Override
    public void removeStorage(Address address, byte[] key) {
        throw new AssertionError("This class does not implement this method.");
    }

    @Override
    public KernelInterface makeChildKernelInterface() {
        return new Substate(this, isLocalCall);
    }

    @Override
    public byte[] getObjectGraph(Address a) {
        if (this.objectGraphs.get(a) == null) {
            if (Constants.DEBUG) {
                System.out.println("JNI: try updating object graph");
            }
            byte[] graph = parent.getObjectGraph(a);
            this.objectGraphs.put(a, graph);
            return graph;
        }

        return this.objectGraphs.get(a);
    }

    @Override
    public void putObjectGraph(Address a, byte[] data) {
        if (Constants.DEBUG) {
            System.out.printf("JNI: save object graph at ");
            System.out.println(a);
        }
        this.objectGraphs.put(a, data);
        Consumer<KernelInterface> write = (kernel) -> {
            kernel.putObjectGraph(a, data);
        };
        writeLog.add(write);

    }

    // @Override
    // public Set<byte[]> getTouchedAccounts() {
    //     throw new AssertionError("This class does not implement this method.");
    // }

    @Override
    public void commitTo(KernelInterface target) { }

    @Override
    public void commit() {
        for (Consumer<KernelInterface> mutation : this.writeLog) {
            mutation.accept(this.parent);
        }
    }

    // Camus: this should not be in kernel interface
    @Override
    public Address getMinerAddress() {
        if (Constants.DEBUG) {
            System.out.printf("JNI: try to get miner address\n");
        }
        
        return this.info.coinbase;
    }

    // Camus: this should not be in kernel interface
    @Override
    public long getBlockDifficulty() {
        return this.info.blockDifficulty;
    }

    // Camus: this should not be in kernel interface
    @Override
    public long getBlockEnergyLimit() {
        return this.info.blockGasLimit;
    }

    // Camus: this should not be in kernel interface
    @Override
    public long getBlockTimestamp() {
        return this.info.blockTimestamp;
    }

    // Camus: this should not be in kernel interface
    @Override
    public long getBlockNumber() {
        return this.info.blockNumber;
    }

    @Override
    public void setTransformedCode(Address address, byte[] bytes) {
        Consumer<KernelInterface> write = (kernel) -> {
            kernel.setTransformedCode(address, bytes);
        };
        writeLog.add(write);
    }

    @Override
    public byte[] getTransformedCode(Address address) {
        return parent.getTransformedCode(address);
    }
}