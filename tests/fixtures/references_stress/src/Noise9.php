<?php

namespace App;

class Noise9
{
    private function compute(): int
    {
        return 9;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
