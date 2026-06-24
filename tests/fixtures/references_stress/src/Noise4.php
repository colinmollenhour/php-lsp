<?php

namespace App;

class Noise4
{
    private function compute(): int
    {
        return 4;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
