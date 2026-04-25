// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract CborWebToken is ERC20, Ownable {
    uint256 public constant TOTAL_SUPPLY = 100_000_000 * 1 ether;
    uint256 public constant MIN_HOLD = 1 ether;

    constructor() ERC20("CBOR-Web Token", "CBORW") Ownable(msg.sender) {
        _mint(msg.sender, TOTAL_SUPPLY);
    }

    function verifyAccess(address holder) external view returns (bool, uint256) {
        uint256 bal = balanceOf(holder);
        return (bal >= MIN_HOLD, bal);
    }
}
